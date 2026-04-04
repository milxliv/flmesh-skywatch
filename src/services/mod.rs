use std::sync::Arc;
use tokio::sync::Mutex;
use crate::db;
use crate::pollers::Poller;

/// Runs a single poller in a loop at its configured interval
pub async fn run_poller(
    poller: Arc<dyn Poller>,
    db_conn: Arc<Mutex<rusqlite::Connection>>,
    client: reqwest::Client,
) {
    let interval = std::time::Duration::from_secs(poller.interval_secs());
    let source = poller.source_key().to_string();
    let name = poller.name().to_string();

    tracing::info!("Starting poller: {} (every {}s)", name, poller.interval_secs());

    loop {
        tracing::debug!("Polling: {}", name);

        match poller.poll(&client).await {
            Ok(events) => {
                let count = events.len();
                let active_ids: Vec<String> = events.iter().map(|e| e.source_id.clone()).collect();

                let conn = db_conn.lock().await;

                let mut inserted = 0;
                for event in &events {
                    match db::upsert_event(&conn, event) {
                        Ok(true) => inserted += 1,
                        Ok(false) => {}
                        Err(e) => {
                            tracing::error!("DB upsert error for {}/{}: {}", source, event.source_id, e);
                        }
                    }
                }

                // Expire events no longer in the active feed
                match db::expire_events(&conn, &source, &active_ids) {
                    Ok(expired) => {
                        if expired > 0 {
                            tracing::info!("{}: expired {} events", name, expired);
                        }
                    }
                    Err(e) => tracing::error!("{}: expire error: {}", name, e),
                }

                // Refresh stats
                let _ = db::refresh_stats(&conn);

                // Update feed health
                let _ = db::update_feed_status(&conn, &source, true, None);

                tracing::info!(
                    "{}: {} active events ({} new/updated)",
                    name, count, inserted
                );
            }
            Err(e) => {
                tracing::error!("Poller {} error: {}", name, e);
                let conn = db_conn.lock().await;
                let _ = db::update_feed_status(&conn, &source, false, Some(&e.to_string()));
            }
        }

        tokio::time::sleep(interval).await;
    }
}
