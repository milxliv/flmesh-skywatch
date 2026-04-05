pub mod mesh_nodes;
pub mod nws_alerts;
pub mod usgs_quakes;

use crate::error::AppError;
use crate::models::Event;

/// Every data feed implements this trait.
/// The poller engine calls `poll()` on a schedule and feeds results into the DB.
#[async_trait::async_trait]
pub trait Poller: Send + Sync {
    /// Human-readable name of this feed
    fn name(&self) -> &str;

    /// Source key used in the events table (e.g., "nws", "usgs")
    fn source_key(&self) -> &str;

    /// How often to poll, in seconds
    fn interval_secs(&self) -> u64;

    /// Fetch and normalize events from the upstream feed
    async fn poll(&self, client: &reqwest::Client) -> Result<Vec<Event>, AppError>;
}
