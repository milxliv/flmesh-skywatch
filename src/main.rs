mod config;
mod db;
mod error;
mod models;
mod pollers;
mod routes;
mod services;

use axum::{routing::get, Router};
use clap::Parser;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::services::ServeDir;

use crate::config::Config;
use crate::pollers::nws_alerts::NwsAlertsPoller;
use crate::pollers::Poller;
use crate::routes::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::parse();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(&config.log_level))
        .init();

    tracing::info!("SkyWatch v{}", env!("CARGO_PKG_VERSION"));
    tracing::info!("Database: {}", config.database);

    // Initialize database
    let conn = db::init_db(&config.database)?;
    let db_conn = Arc::new(Mutex::new(conn));

    // HTTP client shared by all pollers
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    // Register pollers
    let mut _poller_handles = Vec::new();

    if !config.no_nws {
        let poller: Arc<dyn Poller> = Arc::new(NwsAlertsPoller::new());
        let db_clone = Arc::clone(&db_conn);
        let client_clone = client.clone();
        let handle = tokio::spawn(services::run_poller(poller, db_clone, client_clone));
        _poller_handles.push(handle);
    }

    // Future pollers go here:
    // if !config.no_usgs { ... }
    // if !config.no_airnow { ... }

    tracing::info!("Started {} pollers", _poller_handles.len());

    // Shared state for Axum
    let state = Arc::new(AppState {
        db: Mutex::new(db::init_db(&config.database)?),
    });

    // Routes
    let app = Router::new()
        // Dashboard
        .route("/", get(routes::index))
        // HTMX partials
        .route("/api/events", get(routes::events_feed))
        .route("/api/stats", get(routes::stats_panel))
        .route("/api/health", get(routes::feed_health))
        // JSON endpoints
        .route("/api/map", get(routes::map_data))
        // Health check
        .route("/api/ping", get(|| async { "ok" }))
        // Static files
        .nest_service("/static", ServeDir::new("static"))
        .with_state(state);

    let addr = format!("{}:{}", config.address, config.port);
    tracing::info!("Dashboard: http://{addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
