# SkyWatch

Nationwide real-time situational awareness dashboard. Polls live federal data feeds and displays events on an interactive map with severity-coded markers, a scrolling event feed, and aggregate statistics.

**Stack:** Rust / Axum / SQLite WAL / HTMX / Leaflet

## Quick Start

### 1. Download static assets (one-time)

SkyWatch bundles all dependencies locally — no CDN calls at runtime.

```bash
cd static/

# HTMX
curl -L -o htmx.min.js https://unpkg.com/htmx.org@2.0.4/dist/htmx.min.js

# Leaflet
curl -L -o leaflet.js https://unpkg.com/leaflet@1.9.4/dist/leaflet.js
curl -L -o leaflet.css https://unpkg.com/leaflet@1.9.4/dist/leaflet.css

cd ..
```

### 2. Build & Run

```bash
cargo build --release
./target/release/skywatch
```

Dashboard: **http://localhost:3005**

### CLI Options

```
skywatch [OPTIONS]

  -d, --database <PATH>    SQLite path [default: ./data/skywatch.db]
  -a, --address <ADDR>     Listen address [default: 0.0.0.0]
  -p, --port <PORT>        Listen port [default: 3005]
      --no-nws             Disable NWS alert poller
      --log-level <LEVEL>  Log level [default: info]
```

## Architecture

```
┌─────────────────────────────────────────────┐
│                  SkyWatch                    │
│                                              │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  │
│  │   NWS    │  │   USGS   │  │  AirNow  │  │  ← Pollers (async, scheduled)
│  │  Alerts  │  │  Quakes  │  │   AQI    │  │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  │
│       │              │              │        │
│       └──────────────┼──────────────┘        │
│                      ▼                       │
│              ┌──────────────┐                │
│              │ Normalized   │                │
│              │ Event Model  │                │  ← SQLite WAL
│              │  (SQLite)    │                │
│              └──────┬───────┘                │
│                     │                        │
│              ┌──────┴───────┐                │
│              │  Axum + HTMX │                │  ← Single binary web server
│              │  Dashboard   │                │
│              └──────────────┘                │
└─────────────────────────────────────────────┘
```

## Data Feeds

### Live (v0.1)
- **NWS Active Alerts** — `api.weather.gov/alerts/active` (60s interval)
  - Tornado warnings, severe thunderstorm, flood, heat, winter storm, etc.
  - Nationwide, no API key required

### Planned
- **USGS Earthquakes** — `earthquake.usgs.gov` real-time GeoJSON
- **EPA AirNow** — Air Quality Index by station
- **USGS Water Services** — Stream gauge data
- **NOAA Space Weather** — Solar/geomagnetic events
- **NIFC Wildfires** — Active fire incidents

## Adding a New Feed

1. Create `src/pollers/your_feed.rs`
2. Implement the `Poller` trait:
   - `name()` — display name
   - `source_key()` — DB identifier (e.g., "usgs")
   - `interval_secs()` — poll frequency
   - `poll()` — fetch, parse, return `Vec<Event>`
3. Register in `main.rs` alongside the NWS poller
4. That's it — the poller engine handles scheduling, DB upsert, expiration, and stats

## Port Assignment

SkyWatch uses port **3005** (fits Adam's port allocation: GK=3000, HSCA=3001, Chronicle=3002, CW=3003, FCP=3004).

## License

Private / Internal
