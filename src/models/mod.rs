use serde::{Deserialize, Serialize};

/// Severity levels, ordered from most to least critical
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Extreme,
    Severe,
    Moderate,
    Minor,
    Unknown,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Extreme => "extreme",
            Severity::Severe => "severe",
            Severity::Moderate => "moderate",
            Severity::Minor => "minor",
            Severity::Unknown => "unknown",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "extreme" => Severity::Extreme,
            "severe" => Severity::Severe,
            "moderate" => Severity::Moderate,
            "minor" => Severity::Minor,
            _ => Severity::Unknown,
        }
    }

    /// CSS class for dashboard rendering
    pub fn css_class(&self) -> &'static str {
        match self {
            Severity::Extreme => "severity-extreme",
            Severity::Severe => "severity-severe",
            Severity::Moderate => "severity-moderate",
            Severity::Minor => "severity-minor",
            Severity::Unknown => "severity-unknown",
        }
    }

    /// Map marker color
    pub fn color(&self) -> &'static str {
        match self {
            Severity::Extreme => "#dc2626",  // red-600
            Severity::Severe => "#ea580c",   // orange-600
            Severity::Moderate => "#ca8a04",  // yellow-600
            Severity::Minor => "#2563eb",    // blue-600
            Severity::Unknown => "#6b7280",  // gray-500
        }
    }
}

/// A normalized event that every poller produces.
/// This is what goes into SQLite and what the dashboard renders.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub source: String,
    pub source_id: String,
    pub event_type: String,
    pub severity: Severity,
    pub title: String,
    pub description: Option<String>,
    pub url: Option<String>,

    // Temporal
    pub onset_at: Option<String>,
    pub expires_at: Option<String>,
    pub detected_at: String,

    // Geospatial
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub area_desc: Option<String>,
    pub geometry_json: Option<String>,

    // Source-specific extras
    pub metadata: serde_json::Value,

    pub is_active: bool,
}

/// Dashboard statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DashboardStats {
    pub total_active: i64,
    pub extreme_count: i64,
    pub severe_count: i64,
    pub moderate_count: i64,
    pub minor_count: i64,
    pub by_source: Vec<SourceCount>,
    pub by_type: Vec<TypeCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceCount {
    pub source: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeCount {
    pub event_type: String,
    pub count: i64,
}

/// Feed health info for the status panel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedHealth {
    pub source: String,
    pub last_poll_at: Option<String>,
    pub last_success_at: Option<String>,
    pub last_error: Option<String>,
    pub event_count: i64,
    pub poll_count: i64,
    pub error_count: i64,
}
