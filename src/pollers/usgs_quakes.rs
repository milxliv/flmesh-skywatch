use crate::error::AppError;
use crate::models::{Event, Severity};
use crate::pollers::Poller;

/// USGS Earthquake Hazards Program — real-time GeoJSON feed
/// M2.5+ past day: updates every ~5 minutes on the USGS side
const USGS_QUAKES_URL: &str =
    "https://earthquake.usgs.gov/earthquakes/feed/v1.0/summary/2.5_day.geojson";
const USER_AGENT: &str = "SkyWatch/0.1 (situational-awareness-dashboard)";

pub struct UsgsQuakesPoller;

impl UsgsQuakesPoller {
    pub fn new() -> Self {
        Self
    }

    /// Map earthquake magnitude to severity
    fn mag_to_severity(mag: f64) -> Severity {
        if mag >= 7.0 {
            Severity::Extreme
        } else if mag >= 5.0 {
            Severity::Severe
        } else if mag >= 4.0 {
            Severity::Moderate
        } else {
            Severity::Minor
        }
    }
}

#[async_trait::async_trait]
impl Poller for UsgsQuakesPoller {
    fn name(&self) -> &str {
        "USGS Earthquakes"
    }

    fn source_key(&self) -> &str {
        "usgs"
    }

    fn interval_secs(&self) -> u64 {
        300 // 5 minutes — matches USGS update cadence
    }

    async fn poll(&self, client: &reqwest::Client) -> Result<Vec<Event>, AppError> {
        let response = client
            .get(USGS_QUAKES_URL)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "application/json")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(AppError::Poller {
                poller: "usgs".to_string(),
                message: format!("HTTP {}", response.status()),
            });
        }

        let body: serde_json::Value = response.json().await?;
        let features = body
            .get("features")
            .and_then(|f| f.as_array())
            .ok_or_else(|| AppError::Poller {
                poller: "usgs".to_string(),
                message: "No features array in response".to_string(),
            })?;

        let now = chrono::Utc::now().to_rfc3339();
        let mut events = Vec::with_capacity(features.len());

        for feature in features {
            let props = match feature.get("properties") {
                Some(p) => p,
                None => continue,
            };

            let source_id = match feature.get("id").and_then(|v| v.as_str()) {
                Some(id) => id.to_string(),
                None => continue,
            };

            let mag = props.get("mag").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let title = props
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown Earthquake")
                .to_string();
            let place = props.get("place").and_then(|v| v.as_str()).map(|s| s.to_string());

            // Coordinates: [longitude, latitude, depth]
            let (latitude, longitude) = feature
                .get("geometry")
                .and_then(|g| g.get("coordinates"))
                .and_then(|c| c.as_array())
                .and_then(|arr| {
                    let lng = arr.first().and_then(|v| v.as_f64())?;
                    let lat = arr.get(1).and_then(|v| v.as_f64())?;
                    Some((lat, lng))
                })
                .unwrap_or((0.0, 0.0));

            let depth = feature
                .get("geometry")
                .and_then(|g| g.get("coordinates"))
                .and_then(|c| c.get(2))
                .and_then(|v| v.as_f64());

            // Time is in milliseconds since epoch
            let onset_at = props.get("time").and_then(|v| v.as_i64()).map(|ms| {
                chrono::DateTime::from_timestamp(ms / 1000, ((ms % 1000) * 1_000_000) as u32)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_default()
            });

            let geometry_json = feature
                .get("geometry")
                .map(|g| serde_json::to_string(g).unwrap_or_default());

            let severity = Self::mag_to_severity(mag);

            let metadata = serde_json::json!({
                "mag": mag,
                "mag_type": props.get("magType").and_then(|v| v.as_str()),
                "depth_km": depth,
                "felt": props.get("felt").and_then(|v| v.as_i64()),
                "tsunami": props.get("tsunami").and_then(|v| v.as_i64()),
                "sig": props.get("sig").and_then(|v| v.as_i64()),
                "alert": props.get("alert").and_then(|v| v.as_str()),
                "status": props.get("status").and_then(|v| v.as_str()),
            });

            let url = props.get("url").and_then(|v| v.as_str()).map(|s| s.to_string());

            let id = ulid::Ulid::new().to_string();

            events.push(Event {
                id,
                source: "usgs".to_string(),
                source_id,
                event_type: "earthquake".to_string(),
                severity,
                title,
                description: place.clone(),
                url,
                onset_at,
                expires_at: None,
                detected_at: now.clone(),
                latitude: Some(latitude),
                longitude: Some(longitude),
                area_desc: place,
                geometry_json,
                metadata,
                is_active: true,
            });
        }

        tracing::info!("USGS poller: parsed {} earthquakes", events.len());
        Ok(events)
    }
}
