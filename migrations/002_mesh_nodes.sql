-- FL Mesh: Meshtastic node positions cached from FL Mesh API

CREATE TABLE IF NOT EXISTS mesh_nodes (
    node_id         TEXT PRIMARY KEY,       -- Meshtastic node ID (e.g. "3945187926")
    node_id_hex     TEXT,                   -- Hex form (e.g. "!eb26ca56")
    long_name       TEXT NOT NULL,
    short_name      TEXT,
    hardware_model  TEXT,                   -- Human-readable (e.g. "RAK4631")
    role            TEXT,                   -- e.g. "ROUTER", "CLIENT"
    firmware_version TEXT,
    latitude        REAL,                   -- Decimal degrees (API sends int / 1e7)
    longitude       REAL,
    altitude        REAL,
    battery_level   INTEGER,               -- 0-100, 101 = plugged in
    uptime_seconds  INTEGER,
    is_online       INTEGER NOT NULL DEFAULT 0,
    last_heard_at   TEXT,                   -- ISO8601 from API updated_at
    fetched_at      TEXT NOT NULL,          -- When we last cached this
    metadata        TEXT NOT NULL DEFAULT '{}'  -- JSON for extra fields
);

CREATE INDEX IF NOT EXISTS idx_mesh_nodes_online ON mesh_nodes(is_online);
CREATE INDEX IF NOT EXISTS idx_mesh_nodes_geo ON mesh_nodes(latitude, longitude);
