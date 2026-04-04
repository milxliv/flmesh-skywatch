-- SkyWatch: Normalized event model for all data feeds
-- Every poller normalizes its data into this common schema

CREATE TABLE IF NOT EXISTS events (
    id              TEXT PRIMARY KEY,           -- ULID
    source          TEXT NOT NULL,              -- 'nws', 'usgs', 'airnow', etc.
    source_id       TEXT NOT NULL,              -- Original ID from the feed
    event_type      TEXT NOT NULL,              -- 'weather_alert', 'earthquake', 'air_quality', etc.
    severity        TEXT NOT NULL DEFAULT 'unknown', -- 'extreme', 'severe', 'moderate', 'minor', 'unknown'
    title           TEXT NOT NULL,
    description     TEXT,
    url             TEXT,                       -- Link back to source

    -- Temporal
    onset_at        TEXT,                       -- ISO8601 when event starts/started
    expires_at      TEXT,                       -- ISO8601 when event expires (if applicable)
    detected_at     TEXT NOT NULL,              -- ISO8601 when we first ingested this event

    -- Geospatial (nullable — some events are zone-based, not point-based)
    latitude        REAL,
    longitude       REAL,
    area_desc       TEXT,                       -- Human-readable location (e.g., "Lee County, FL")
    geometry_json   TEXT,                       -- GeoJSON polygon/point for map rendering

    -- Source-specific metadata stored as JSON
    metadata        TEXT NOT NULL DEFAULT '{}',

    -- Housekeeping
    is_active       INTEGER NOT NULL DEFAULT 1, -- 1 = current, 0 = expired/superseded
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),

    UNIQUE(source, source_id)
);

CREATE INDEX IF NOT EXISTS idx_events_source ON events(source);
CREATE INDEX IF NOT EXISTS idx_events_active ON events(is_active);
CREATE INDEX IF NOT EXISTS idx_events_severity ON events(severity);
CREATE INDEX IF NOT EXISTS idx_events_onset ON events(onset_at);
CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
CREATE INDEX IF NOT EXISTS idx_events_geo ON events(latitude, longitude);

-- Feed health tracking
CREATE TABLE IF NOT EXISTS feed_status (
    source          TEXT PRIMARY KEY,
    last_poll_at    TEXT,
    last_success_at TEXT,
    last_error      TEXT,
    event_count     INTEGER NOT NULL DEFAULT 0, -- Total events ingested all-time
    poll_count      INTEGER NOT NULL DEFAULT 0,
    error_count     INTEGER NOT NULL DEFAULT 0
);

-- Severity stats cache (updated by triggers or periodic refresh)
CREATE TABLE IF NOT EXISTS stats_snapshot (
    id              INTEGER PRIMARY KEY CHECK (id = 1), -- Singleton row
    total_active    INTEGER NOT NULL DEFAULT 0,
    extreme_count   INTEGER NOT NULL DEFAULT 0,
    severe_count    INTEGER NOT NULL DEFAULT 0,
    moderate_count  INTEGER NOT NULL DEFAULT 0,
    minor_count     INTEGER NOT NULL DEFAULT 0,
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

INSERT OR IGNORE INTO stats_snapshot (id) VALUES (1);
