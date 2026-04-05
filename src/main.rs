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
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;

use crate::config::Config;
use crate::pollers::nws_alerts::NwsAlertsPoller;
use crate::pollers::usgs_quakes::UsgsQuakesPoller;
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

    if !config.no_usgs {
        let poller: Arc<dyn Poller> = Arc::new(UsgsQuakesPoller::new());
        let db_clone = Arc::clone(&db_conn);
        let client_clone = client.clone();
        let handle = tokio::spawn(services::run_poller(poller, db_clone, client_clone));
        _poller_handles.push(handle);
    }

    if !config.no_mesh {
        let db_clone = Arc::clone(&db_conn);
        let client_clone = client.clone();
        let handle = tokio::spawn(pollers::mesh_nodes::run_mesh_poller(db_clone, client_clone));
        _poller_handles.push(handle);
    }

    tracing::info!("Started {} pollers", _poller_handles.len());

    // Shared state for Axum
    let state = Arc::new(AppState {
        db: Mutex::new(db::init_db(&config.database)?),
    });

    // CORS: allow known FL Mesh origins and local dev
    let cors = CorsLayer::new()
        .allow_origin([
            "https://flmesh-proposal.pages.dev".parse().expect("valid origin"),
            "https://areyoumeshingwith.us".parse().expect("valid origin"),
            "https://www.areyoumeshingwith.us".parse().expect("valid origin"),
            "http://localhost:3005".parse().expect("valid origin"),
            "http://localhost:8080".parse().expect("valid origin"),
            "http://127.0.0.1:8080".parse().expect("valid origin"),
        ])
        .allow_methods(Any)
        .allow_headers(Any);

    // Routes
    let app = Router::new()
        // Dashboard
        .route("/", get(routes::index))
        .route("/map", get(routes::map_page))
        // HTMX partials
        .route("/api/events", get(routes::events_feed))
        .route("/api/stats", get(routes::stats_panel))
        .route("/api/health", get(routes::feed_health))
        // JSON endpoints
        .route("/api/map", get(routes::map_data))
        .route("/api/mesh-nodes", get(routes::mesh_nodes))
        // Health check
        .route("/api/ping", get(|| async { "ok" }))
        // Static files
        .nest_service("/static", ServeDir::new("static"))
        .layer(cors)
        .with_state(state);

    let addr = format!("{}:{}", config.address, config.port);
    tracing::info!("Dashboard: http://{addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
