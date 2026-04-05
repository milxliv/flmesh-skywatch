use rusqlite::{params, Connection};
use crate::error::AppError;
use crate::models::{DashboardStats, Event, FeedHealth, Severity, SourceCount, TypeCount};
use crate::pollers::mesh_nodes::MeshNode;

pub fn init_db(path: &str) -> Result<Connection, AppError> {
    // Ensure parent directory exists
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    let conn = Connection::open(path)?;

    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;",
    )?;

    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<(), AppError> {
    let sql = include_str!("../migrations/001_initial.sql");
    conn.execute_batch(sql)?;
    let sql2 = include_str!("../migrations/002_mesh_nodes.sql");
    conn.execute_batch(sql2)?;
    Ok(())
}

/// Upsert an event. Returns true if this was a new insert, false if update.
pub fn upsert_event(conn: &Connection, event: &Event) -> Result<bool, AppError> {
    let rows = conn.execute(
        "INSERT INTO events (
            id, source, source_id, event_type, severity, title, description, url,
            onset_at, expires_at, detected_at,
            latitude, longitude, area_desc, geometry_json,
            metadata, is_active
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
        ON CONFLICT(source, source_id) DO UPDATE SET
            severity = excluded.severity,
            title = excluded.title,
            description = excluded.description,
            expires_at = excluded.expires_at,
            metadata = excluded.metadata,
            is_active = excluded.is_active,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        params![
            event.id,
            event.source,
            event.source_id,
            event.event_type,
            event.severity.as_str(),
            event.title,
            event.description,
            event.url,
            event.onset_at,
            event.expires_at,
            event.detected_at,
            event.latitude,
            event.longitude,
            event.area_desc,
            event.geometry_json,
            event.metadata.to_string(),
            event.is_active as i32,
        ],
    )?;
    Ok(rows > 0)
}

/// Mark events from a source as inactive if they aren't in the active set
pub fn expire_events(conn: &Connection, source: &str, active_source_ids: &[String]) -> Result<usize, AppError> {
    if active_source_ids.is_empty() {
        let count = conn.execute(
            "UPDATE events SET is_active = 0 WHERE source = ?1 AND is_active = 1",
            params![source],
        )?;
        return Ok(count);
    }

    // Build placeholders for IN clause
    let placeholders: Vec<String> = active_source_ids.iter().enumerate().map(|(i, _)| format!("?{}", i + 2)).collect();
    let sql = format!(
        "UPDATE events SET is_active = 0 WHERE source = ?1 AND is_active = 1 AND source_id NOT IN ({})",
        placeholders.join(", ")
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    param_values.push(Box::new(source.to_string()));
    for id in active_source_ids {
        param_values.push(Box::new(id.clone()));
    }
    let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
    let count = stmt.execute(params_refs.as_slice())?;
    Ok(count)
}

/// Update feed health after a poll
pub fn update_feed_status(
    conn: &Connection,
    source: &str,
    success: bool,
    error_msg: Option<&str>,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    if success {
        conn.execute(
            "INSERT INTO feed_status (source, last_poll_at, last_success_at, poll_count)
             VALUES (?1, ?2, ?2, 1)
             ON CONFLICT(source) DO UPDATE SET
                last_poll_at = ?2,
                last_success_at = ?2,
                poll_count = poll_count + 1",
            params![source, now],
        )?;
    } else {
        conn.execute(
            "INSERT INTO feed_status (source, last_poll_at, last_error, poll_count, error_count)
             VALUES (?1, ?2, ?3, 1, 1)
             ON CONFLICT(source) DO UPDATE SET
                last_poll_at = ?2,
                last_error = ?3,
                poll_count = poll_count + 1,
                error_count = error_count + 1",
            params![source, now, error_msg],
        )?;
    }
    Ok(())
}

/// Refresh the stats snapshot
pub fn refresh_stats(conn: &Connection) -> Result<(), AppError> {
    conn.execute(
        "UPDATE stats_snapshot SET
            total_active = (SELECT COUNT(*) FROM events WHERE is_active = 1),
            extreme_count = (SELECT COUNT(*) FROM events WHERE is_active = 1 AND severity = 'extreme'),
            severe_count = (SELECT COUNT(*) FROM events WHERE is_active = 1 AND severity = 'severe'),
            moderate_count = (SELECT COUNT(*) FROM events WHERE is_active = 1 AND severity = 'moderate'),
            minor_count = (SELECT COUNT(*) FROM events WHERE is_active = 1 AND severity = 'minor'),
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE id = 1",
        [],
    )?;
    Ok(())
}

/// Get current dashboard stats
pub fn get_stats(conn: &Connection) -> Result<DashboardStats, AppError> {
    let mut stats = conn.query_row(
        "SELECT total_active, extreme_count, severe_count, moderate_count, minor_count
         FROM stats_snapshot WHERE id = 1",
        [],
        |row| {
            Ok(DashboardStats {
                total_active: row.get(0)?,
                extreme_count: row.get(1)?,
                severe_count: row.get(2)?,
                moderate_count: row.get(3)?,
                minor_count: row.get(4)?,
                by_source: Vec::new(),
                by_type: Vec::new(),
            })
        },
    )?;

    // By source
    let mut stmt = conn.prepare(
        "SELECT source, COUNT(*) FROM events WHERE is_active = 1 GROUP BY source ORDER BY COUNT(*) DESC",
    )?;
    stats.by_source = stmt
        .query_map([], |row| {
            Ok(SourceCount {
                source: row.get(0)?,
                count: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    // By type
    let mut stmt = conn.prepare(
        "SELECT event_type, COUNT(*) FROM events WHERE is_active = 1 GROUP BY event_type ORDER BY COUNT(*) DESC",
    )?;
    stats.by_type = stmt
        .query_map([], |row| {
            Ok(TypeCount {
                event_type: row.get(0)?,
                count: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(stats)
}

/// Get recent active events for the feed view
pub fn get_active_events(
    conn: &Connection,
    limit: usize,
    source_filter: Option<&str>,
    severity_filter: Option<&str>,
) -> Result<Vec<Event>, AppError> {
    let mut sql = String::from(
        "SELECT id, source, source_id, event_type, severity, title, description, url,
                onset_at, expires_at, detected_at,
                latitude, longitude, area_desc, geometry_json,
                metadata, is_active
         FROM events WHERE is_active = 1",
    );

    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut param_idx = 1;

    if let Some(src) = source_filter {
        sql.push_str(&format!(" AND source = ?{param_idx}"));
        param_values.push(Box::new(src.to_string()));
        param_idx += 1;
    }
    if let Some(sev) = severity_filter {
        sql.push_str(&format!(" AND severity = ?{param_idx}"));
        param_values.push(Box::new(sev.to_string()));
        // param_idx += 1; // uncomment when adding more filters
    }

    sql.push_str(&format!(" ORDER BY onset_at DESC LIMIT {limit}"));

    let mut stmt = conn.prepare(&sql)?;
    let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();

    let events = stmt
        .query_map(params_refs.as_slice(), |row| {
            let severity_str: String = row.get(4)?;
            let metadata_str: String = row.get(15)?;
            let is_active_int: i32 = row.get(16)?;
            Ok(Event {
                id: row.get(0)?,
                source: row.get(1)?,
                source_id: row.get(2)?,
                event_type: row.get(3)?,
                severity: Severity::from_str(&severity_str),
                title: row.get(5)?,
                description: row.get(6)?,
                url: row.get(7)?,
                onset_at: row.get(8)?,
                expires_at: row.get(9)?,
                detected_at: row.get(10)?,
                latitude: row.get(11)?,
                longitude: row.get(12)?,
                area_desc: row.get(13)?,
                geometry_json: row.get(14)?,
                metadata: serde_json::from_str(&metadata_str).unwrap_or_default(),
                is_active: is_active_int == 1,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(events)
}

/// Get events with coordinates for map rendering (returns lightweight JSON)
pub fn get_map_events(conn: &Connection) -> Result<Vec<serde_json::Value>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, source, event_type, severity, title, latitude, longitude, area_desc, onset_at
         FROM events
         WHERE is_active = 1 AND latitude IS NOT NULL AND longitude IS NOT NULL
         ORDER BY onset_at DESC
         LIMIT 2000",
    )?;

    let events = stmt
        .query_map([], |row| {
            let severity: String = row.get(3)?;
            let color = Severity::from_str(&severity).color().to_string();
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "source": row.get::<_, String>(1)?,
                "type": row.get::<_, String>(2)?,
                "severity": severity,
                "title": row.get::<_, String>(4)?,
                "lat": row.get::<_, f64>(5)?,
                "lng": row.get::<_, f64>(6)?,
                "area": row.get::<_, Option<String>>(7)?,
                "onset": row.get::<_, Option<String>>(8)?,
                "color": color,
            }))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(events)
}

/// Upsert a mesh node
pub fn upsert_mesh_node(conn: &Connection, node: &MeshNode) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO mesh_nodes (
            node_id, node_id_hex, long_name, short_name, hardware_model, role,
            firmware_version, latitude, longitude, altitude,
            battery_level, uptime_seconds, is_online, last_heard_at, fetched_at, metadata
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
        ON CONFLICT(node_id) DO UPDATE SET
            node_id_hex = excluded.node_id_hex,
            long_name = excluded.long_name,
            short_name = excluded.short_name,
            hardware_model = excluded.hardware_model,
            role = excluded.role,
            firmware_version = excluded.firmware_version,
            latitude = excluded.latitude,
            longitude = excluded.longitude,
            altitude = excluded.altitude,
            battery_level = excluded.battery_level,
            uptime_seconds = excluded.uptime_seconds,
            is_online = excluded.is_online,
            last_heard_at = excluded.last_heard_at,
            fetched_at = excluded.fetched_at,
            metadata = excluded.metadata",
        params![
            node.node_id,
            node.node_id_hex,
            node.long_name,
            node.short_name,
            node.hardware_model,
            node.role,
            node.firmware_version,
            node.latitude,
            node.longitude,
            node.altitude,
            node.battery_level,
            node.uptime_seconds,
            node.is_online as i32,
            node.last_heard_at,
            node.fetched_at,
            node.metadata.to_string(),
        ],
    )?;
    Ok(())
}

/// Get mesh nodes with coordinates for map rendering
pub fn get_mesh_nodes(conn: &Connection) -> Result<Vec<serde_json::Value>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT node_id, node_id_hex, long_name, short_name, hardware_model, role,
                latitude, longitude, altitude, battery_level, uptime_seconds,
                is_online, last_heard_at, metadata
         FROM mesh_nodes
         WHERE latitude IS NOT NULL AND longitude IS NOT NULL
         ORDER BY is_online DESC, last_heard_at DESC",
    )?;

    let nodes = stmt
        .query_map([], |row| {
            let is_online: i32 = row.get(11)?;
            let metadata_str: String = row.get(13)?;
            let metadata: serde_json::Value =
                serde_json::from_str(&metadata_str).unwrap_or_default();
            Ok(serde_json::json!({
                "node_id": row.get::<_, String>(0)?,
                "node_id_hex": row.get::<_, Option<String>>(1)?,
                "long_name": row.get::<_, String>(2)?,
                "short_name": row.get::<_, Option<String>>(3)?,
                "hardware": row.get::<_, Option<String>>(4)?,
                "role": row.get::<_, Option<String>>(5)?,
                "lat": row.get::<_, f64>(6)?,
                "lng": row.get::<_, f64>(7)?,
                "altitude": row.get::<_, Option<f64>>(8)?,
                "battery": row.get::<_, Option<i64>>(9)?,
                "uptime": row.get::<_, Option<i64>>(10)?,
                "online": is_online == 1,
                "last_heard": row.get::<_, Option<String>>(12)?,
                "region": metadata.get("region").and_then(|v| v.as_str()),
                "modem_preset": metadata.get("modem_preset").and_then(|v| v.as_str()),
            }))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(nodes)
}

/// Get feed health for all sources
pub fn get_feed_health(conn: &Connection) -> Result<Vec<FeedHealth>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT source, last_poll_at, last_success_at, last_error, event_count, poll_count, error_count
         FROM feed_status ORDER BY source",
    )?;

    let feeds = stmt
        .query_map([], |row| {
            Ok(FeedHealth {
                source: row.get(0)?,
                last_poll_at: row.get(1)?,
                last_success_at: row.get(2)?,
                last_error: row.get(3)?,
                event_count: row.get(4)?,
                poll_count: row.get(5)?,
                error_count: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(feeds)
}
