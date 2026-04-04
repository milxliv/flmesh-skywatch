use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(name = "skywatch", about = "Nationwide real-time situational awareness dashboard")]
pub struct Config {
    /// SQLite database path
    #[arg(short, long, default_value = "./data/skywatch.db")]
    pub database: String,

    /// Listen address
    #[arg(short, long, default_value = "0.0.0.0")]
    pub address: String,

    /// Listen port
    #[arg(short, long, default_value_t = 3005)]
    pub port: u16,

    /// Disable NWS poller
    #[arg(long, default_value_t = false)]
    pub no_nws: bool,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    pub log_level: String,
}
