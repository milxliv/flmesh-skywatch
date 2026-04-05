use std::sync::Arc;
use tokio::sync::Mutex;

use crate::db;
use crate::error::AppError;

const FLMESH_API_URL: &str = "https://map.areyoumeshingwith.us/api/v1/nodes";
const USER_AGENT: &str = "SkyWatch/0.1 (situational-awareness-dashboard)";

/// How many seconds since last update before a node is considered offline
const OFFLINE_THRESHOLD_SECS: i64 = 3600; // 1 hour

/// Runs the FL Mesh node poller in a loop every 5 minutes.
/// This is a standalone poller (not using the Poller trait) because
/// mesh nodes are device positions, not normalized events.
pub async fn run_mesh_poller(
    db_conn: Arc<Mutex<rusqlite::Connection>>,
    client: reqwest::Client,
) {
    let interval = std::time::Duration::from_secs(300); // 5 minutes

    tracing::info!("Starting poller: FL Mesh Nodes (every 300s)");

    loop {
        tracing::debug!("Polling: FL Mesh Nodes");

        match fetch_nodes(&client).await {
            Ok(nodes) => {
                let count = nodes.len();
                let conn = db_conn.lock().await;

                let mut upserted = 0;
                for node in &nodes {
                    match db::upsert_mesh_node(&conn, node) {
                        Ok(_) => upserted += 1,
                        Err(e) => {
                            tracing::error!(
                                "DB upsert error for mesh node {}: {}",
                                node.node_id,
                                e
                            );
                        }
                    }
                }

                let _ = db::update_feed_status(&conn, "flmesh", true, None);
                tracing::info!("FL Mesh: {} nodes fetched, {} upserted", count, upserted);
            }
            Err(e) => {
                tracing::error!("FL Mesh poller error: {}", e);
                let conn = db_conn.lock().await;
                let _ = db::update_feed_status(&conn, "flmesh", false, Some(&e.to_string()));
            }
        }

        tokio::time::sleep(interval).await;
    }
}

/// A mesh node parsed from the FL Mesh API
pub struct MeshNode {
    pub node_id: String,
    pub node_id_hex: Option<String>,
    pub long_name: String,
    pub short_name: Option<String>,
    pub hardware_model: Option<String>,
    pub role: Option<String>,
    pub firmware_version: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub altitude: Option<f64>,
    pub battery_level: Option<i64>,
    pub uptime_seconds: Option<i64>,
    pub is_online: bool,
    pub last_heard_at: Option<String>,
    pub fetched_at: String,
    pub metadata: serde_json::Value,
}

async fn fetch_nodes(client: &reqwest::Client) -> Result<Vec<MeshNode>, AppError> {
    let response = client
        .get(FLMESH_API_URL)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json")
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(AppError::Poller {
            poller: "flmesh".to_string(),
            message: format!("HTTP {}", response.status()),
        });
    }

    let body: serde_json::Value = response.json().await?;
    let raw_nodes = body
        .get("nodes")
        .and_then(|n| n.as_array())
        .ok_or_else(|| AppError::Poller {
            poller: "flmesh".to_string(),
            message: "No nodes array in response".to_string(),
        })?;

    let now = chrono::Utc::now();
    let now_str = now.to_rfc3339();
    let mut nodes = Vec::with_capacity(raw_nodes.len());

    for raw in raw_nodes {
        // node_id can be string or number in the API
        let node_id = if let Some(s) = raw.get("node_id").and_then(|v| v.as_str()) {
            s.to_string()
        } else if let Some(n) = raw.get("node_id").and_then(|v| v.as_i64()) {
            n.to_string()
        } else if let Some(n) = raw.get("node_id").and_then(|v| v.as_u64()) {
            n.to_string()
        } else {
            continue;
        };

        let long_name = raw
            .get("long_name")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();

        // Coordinates: API sends as integers, divide by 1e7 for decimal degrees
        let latitude = raw.get("latitude").and_then(|v| {
            v.as_f64().or_else(|| v.as_i64().map(|i| i as f64))
        }).map(|v| if v.abs() > 1000.0 { v / 1e7 } else { v });

        let longitude = raw.get("longitude").and_then(|v| {
            v.as_f64().or_else(|| v.as_i64().map(|i| i as f64))
        }).map(|v| if v.abs() > 1000.0 { v / 1e7 } else { v });

        let altitude = raw
            .get("altitude")
            .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64)));

        // Determine online status from updated_at timestamp
        let updated_at = raw
            .get("updated_at")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let is_online = updated_at
            .as_deref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| (now - dt.with_timezone(&chrono::Utc)).num_seconds() < OFFLINE_THRESHOLD_SECS)
            .unwrap_or(false);

        let uptime_str = raw.get("uptime_seconds").and_then(|v| v.as_str());
        let uptime = raw
            .get("uptime_seconds")
            .and_then(|v| v.as_i64().or_else(|| uptime_str.and_then(|s| s.parse().ok())));

        let battery = raw.get("battery_level").and_then(|v| v.as_i64());

        let metadata = serde_json::json!({
            "region": raw.get("region_name").and_then(|v| v.as_str()),
            "modem_preset": raw.get("modem_preset_name").and_then(|v| v.as_str()),
            "channel_utilization": raw.get("channel_utilization").and_then(|v| v.as_str()),
            "air_util_tx": raw.get("air_util_tx").and_then(|v| v.as_str()),
            "num_online_local_nodes": raw.get("num_online_local_nodes").and_then(|v| v.as_i64()),
            "temperature": raw.get("temperature").and_then(|v| v.as_str().or_else(|| None)),
            "is_licensed": raw.get("is_licensed").and_then(|v| v.as_bool()),
        });

        nodes.push(MeshNode {
            node_id,
            node_id_hex: raw.get("node_id_hex").and_then(|v| v.as_str()).map(|s| s.to_string()),
            long_name,
            short_name: raw.get("short_name").and_then(|v| v.as_str()).map(|s| s.to_string()),
            hardware_model: raw
                .get("hardware_model_name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            role: raw.get("role_name").and_then(|v| v.as_str()).map(|s| s.to_string()),
            firmware_version: raw
                .get("firmware_version")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            latitude,
            longitude,
            altitude,
            battery_level: battery,
            uptime_seconds: uptime,
            is_online,
            last_heard_at: updated_at,
            fetched_at: now_str.clone(),
            metadata,
        });
    }

    tracing::info!("FL Mesh: parsed {} nodes", nodes.len());
    Ok(nodes)
}
