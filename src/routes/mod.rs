use axum::extract::{Query, State};
use axum::response::Html;
use axum::Json;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

use crate::db;
use crate::error::AppError;

pub struct AppState {
    pub db: Mutex<rusqlite::Connection>,
    pub http: reqwest::Client,
    pub mesh_cache: Mutex<MeshCache>,
}

pub struct MeshCache {
    pub data: serde_json::Value,
    pub fetched_at: Option<Instant>,
}

const MESH_API_URL: &str = "https://map.areyoumeshingwith.us/api/v1/nodes";
const MESH_CACHE_SECS: u64 = 60;

#[derive(serde::Deserialize, Default)]
pub struct EventFilters {
    pub source: Option<String>,
    pub severity: Option<String>,
    pub limit: Option<usize>,
}

/// Main dashboard page
pub async fn index() -> Html<String> {
    let template = include_str!("../../templates/index.html");
    Html(template.to_string())
}

/// HTMX partial: event feed
pub async fn events_feed(
    State(state): State<Arc<AppState>>,
    Query(filters): Query<EventFilters>,
) -> Result<Html<String>, AppError> {
    let conn = state.db.lock().await;
    let events = db::get_active_events(
        &conn,
        filters.limit.unwrap_or(100),
        filters.source.as_deref(),
        filters.severity.as_deref(),
    )?;

    let mut html = String::with_capacity(events.len() * 500);
    for event in &events {
        let severity_class = event.severity.css_class();
        let source_badge = match event.source.as_str() {
            "nws" => r#"<span class="badge badge-nws">NWS</span>"#,
            "usgs" => r#"<span class="badge badge-usgs">USGS</span>"#,
            _ => r#"<span class="badge">{source}</span>"#,
        };

        let area = event.area_desc.as_deref().unwrap_or("—");
        let onset = event
            .onset_at
            .as_deref()
            .map(|s| {
                // Show just time portion for readability
                if s.len() > 16 { &s[..16] } else { s }
            })
            .unwrap_or("—");

        html.push_str(&format!(
            r#"<div class="event-card {severity_class}">
                <div class="event-header">
                    {source_badge}
                    <span class="event-severity">{severity}</span>
                    <span class="event-time">{onset}</span>
                </div>
                <div class="event-title">{title}</div>
                <div class="event-area">{area}</div>
            </div>"#,
            severity_class = severity_class,
            source_badge = source_badge,
            severity = event.severity.as_str().to_uppercase(),
            onset = onset,
            title = html_escape(&event.title),
            area = html_escape(area),
        ));
    }

    if events.is_empty() {
        html.push_str(r#"<div class="event-card">No active events</div>"#);
    }

    Ok(Html(html))
}

/// HTMX partial: stats panel
pub async fn stats_panel(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, AppError> {
    let conn = state.db.lock().await;
    let stats = db::get_stats(&conn)?;

    let mut html = String::new();
    html.push_str(&format!(
        r#"<div class="stat-grid">
            <div class="stat-card stat-total">
                <div class="stat-number">{}</div>
                <div class="stat-label">Active Events</div>
            </div>
            <div class="stat-card severity-extreme">
                <div class="stat-number">{}</div>
                <div class="stat-label">Extreme</div>
            </div>
            <div class="stat-card severity-severe">
                <div class="stat-number">{}</div>
                <div class="stat-label">Severe</div>
            </div>
            <div class="stat-card severity-moderate">
                <div class="stat-number">{}</div>
                <div class="stat-label">Moderate</div>
            </div>
            <div class="stat-card severity-minor">
                <div class="stat-number">{}</div>
                <div class="stat-label">Minor</div>
            </div>
        </div>"#,
        stats.total_active,
        stats.extreme_count,
        stats.severe_count,
        stats.moderate_count,
        stats.minor_count,
    ));

    // Source breakdown
    if !stats.by_source.is_empty() {
        html.push_str(r#"<div class="stat-section"><h3>By Source</h3>"#);
        for sc in &stats.by_source {
            html.push_str(&format!(
                r#"<div class="stat-row"><span class="stat-key">{}</span><span class="stat-val">{}</span></div>"#,
                sc.source.to_uppercase(),
                sc.count
            ));
        }
        html.push_str("</div>");
    }

    // Type breakdown
    if !stats.by_type.is_empty() {
        html.push_str(r#"<div class="stat-section"><h3>By Type</h3>"#);
        for tc in &stats.by_type {
            html.push_str(&format!(
                r#"<div class="stat-row"><span class="stat-key">{}</span><span class="stat-val">{}</span></div>"#,
                html_escape(&tc.event_type),
                tc.count
            ));
        }
        html.push_str("</div>");
    }

    Ok(Html(html))
}

/// JSON endpoint: map markers for Leaflet
pub async fn map_data(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let conn = state.db.lock().await;
    let events = db::get_map_events(&conn)?;
    Ok(Json(events))
}

/// JSON endpoint: feed health
pub async fn feed_health(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, AppError> {
    let conn = state.db.lock().await;
    let feeds = db::get_feed_health(&conn)?;

    let mut html = String::new();
    for feed in &feeds {
        let status_class = if feed.last_error.is_some() { "feed-error" } else { "feed-ok" };
        let last_ok = feed.last_success_at.as_deref().unwrap_or("never");
        let error_text = feed.last_error.as_deref().unwrap_or("—");

        html.push_str(&format!(
            r#"<div class="feed-card {status_class}">
                <div class="feed-name">{}</div>
                <div class="feed-detail">Last OK: {last_ok}</div>
                <div class="feed-detail">Polls: {} | Errors: {}</div>
                <div class="feed-error-text">{error_text}</div>
            </div>"#,
            feed.source.to_uppercase(),
            feed.poll_count,
            feed.error_count,
        ));
    }

    if feeds.is_empty() {
        html.push_str(r#"<div class="feed-card">No feeds registered yet — waiting for first poll</div>"#);
    }

    Ok(Html(html))
}

/// JSON proxy: FL Mesh nodes (cached 60s)
pub async fn mesh_nodes(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    {
        let cache = state.mesh_cache.lock().await;
        if let Some(t) = cache.fetched_at {
            if t.elapsed().as_secs() < MESH_CACHE_SECS {
                return Ok(Json(cache.data.clone()));
            }
        }
    }

    let resp = state
        .http
        .get(MESH_API_URL)
        .send()
        .await
        .map_err(|e| AppError::Fetch(format!("mesh nodes: {e}")))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::Fetch(format!("mesh nodes parse: {e}")))?;

    let mut cache = state.mesh_cache.lock().await;
    cache.data = body.clone();
    cache.fetched_at = Some(Instant::now());

    Ok(Json(body))
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
