use crate::error::AppError;
use crate::models::{Event, Severity};
use crate::pollers::Poller;

const NWS_ALERTS_URL: &str = "https://api.weather.gov/alerts/active?status=actual";
const USER_AGENT: &str = "SkyWatch/0.1 (situational-awareness-dashboard)";

pub struct NwsAlertsPoller;

impl NwsAlertsPoller {
    pub fn new() -> Self {
        Self
    }

    /// Map NWS severity string to our normalized severity
    fn map_severity(nws_severity: Option<&str>, nws_certainty: Option<&str>) -> Severity {
        match nws_severity {
            Some("Extreme") => Severity::Extreme,
            Some("Severe") => Severity::Severe,
            Some("Moderate") => Severity::Moderate,
            Some("Minor") => Severity::Minor,
            _ => {
                // Fall back to certainty if severity is unknown
                match nws_certainty {
                    Some("Observed") | Some("Likely") => Severity::Moderate,
                    _ => Severity::Unknown,
                }
            }
        }
    }

    /// Determine a centroid from NWS alert geometry or affected zones
    fn extract_coordinates(feature: &serde_json::Value) -> (Option<f64>, Option<f64>, Option<String>) {
        // Try geometry first (polygon alerts)
        if let Some(geometry) = feature.get("geometry") {
            if !geometry.is_null() {
                if let Some(coords) = geometry.get("coordinates") {
                    // For Polygon, compute centroid of first ring
                    if let Some(geom_type) = geometry.get("type").and_then(|t| t.as_str()) {
                        if geom_type == "Polygon" {
                            if let Some(ring) = coords.get(0).and_then(|r| r.as_array()) {
                                let (mut sum_lng, mut sum_lat, mut count) = (0.0, 0.0, 0.0);
                                for point in ring {
                                    if let (Some(lng), Some(lat)) = (
                                        point.get(0).and_then(|v| v.as_f64()),
                                        point.get(1).and_then(|v| v.as_f64()),
                                    ) {
                                        sum_lng += lng;
                                        sum_lat += lat;
                                        count += 1.0;
                                    }
                                }
                                if count > 0.0 {
                                    let geom_json = serde_json::to_string(geometry).ok();
                                    return (Some(sum_lat / count), Some(sum_lng / count), geom_json);
                                }
                            }
                        } else if geom_type == "Point" {
                            if let (Some(lng), Some(lat)) = (
                                coords.get(0).and_then(|v| v.as_f64()),
                                coords.get(1).and_then(|v| v.as_f64()),
                            ) {
                                let geom_json = serde_json::to_string(geometry).ok();
                                return (Some(lat), Some(lng), geom_json);
                            }
                        }
                    }
                }
            }
        }

        // No geometry available — zone-based alert
        (None, None, None)
    }
}

#[async_trait::async_trait]
impl Poller for NwsAlertsPoller {
    fn name(&self) -> &str {
        "NWS Active Alerts"
    }

    fn source_key(&self) -> &str {
        "nws"
    }

    fn interval_secs(&self) -> u64 {
        60 // NWS updates frequently, but be a good citizen
    }

    async fn poll(&self, client: &reqwest::Client) -> Result<Vec<Event>, AppError> {
        let response = client
            .get(NWS_ALERTS_URL)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "application/geo+json")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(AppError::Poller {
                poller: "nws".to_string(),
                message: format!("HTTP {}", response.status()),
            });
        }

        let body: serde_json::Value = response.json().await?;
        let features = body
            .get("features")
            .and_then(|f| f.as_array())
            .ok_or_else(|| AppError::Poller {
                poller: "nws".to_string(),
                message: "No features array in response".to_string(),
            })?;

        let now = chrono::Utc::now().to_rfc3339();
        let mut events = Vec::with_capacity(features.len());

        for feature in features {
            let props = match feature.get("properties") {
                Some(p) => p,
                None => continue,
            };

            let source_id = match props.get("id").and_then(|v| v.as_str()) {
                Some(id) => id.to_string(),
                None => continue,
            };

            let event_name = props.get("event").and_then(|v| v.as_str()).unwrap_or("Unknown Alert");
            let headline = props.get("headline").and_then(|v| v.as_str()).unwrap_or(event_name);
            let description = props.get("description").and_then(|v| v.as_str()).map(|s| {
                // Truncate very long descriptions for storage
                if s.len() > 2000 { format!("{}...", &s[..2000]) } else { s.to_string() }
            });

            let severity = Self::map_severity(
                props.get("severity").and_then(|v| v.as_str()),
                props.get("certainty").and_then(|v| v.as_str()),
            );

            let onset = props.get("onset").and_then(|v| v.as_str())
                .or_else(|| props.get("effective").and_then(|v| v.as_str()))
                .map(|s| s.to_string());

            let expires = props.get("expires").and_then(|v| v.as_str())
                .or_else(|| props.get("ends").and_then(|v| v.as_str()))
                .map(|s| s.to_string());

            let area_desc = props.get("areaDesc").and_then(|v| v.as_str()).map(|s| s.to_string());

            let (latitude, longitude, geometry_json) = Self::extract_coordinates(feature);

            // Build metadata with NWS-specific fields
            let metadata = serde_json::json!({
                "event": event_name,
                "message_type": props.get("messageType").and_then(|v| v.as_str()),
                "category": props.get("category").and_then(|v| v.as_str()),
                "urgency": props.get("urgency").and_then(|v| v.as_str()),
                "certainty": props.get("certainty").and_then(|v| v.as_str()),
                "sender": props.get("senderName").and_then(|v| v.as_str()),
                "geocode_same": props.get("geocode").and_then(|g| g.get("SAME")),
                "geocode_ugc": props.get("geocode").and_then(|g| g.get("UGC")),
            });

            let id = ulid::Ulid::new().to_string();

            events.push(Event {
                id,
                source: "nws".to_string(),
                source_id,
                event_type: "weather_alert".to_string(),
                severity,
                title: headline.to_string(),
                description,
                url: props.get("@id").and_then(|v| v.as_str()).map(|s| s.to_string()),
                onset_at: onset,
                expires_at: expires,
                detected_at: now.clone(),
                latitude,
                longitude,
                area_desc,
                geometry_json,
                metadata,
                is_active: true,
            });
        }

        tracing::info!("NWS poller: parsed {} active alerts", events.len());
        Ok(events)
    }
}
