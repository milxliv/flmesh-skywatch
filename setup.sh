#!/bin/bash
# SkyWatch - Download static dependencies
# Run this once before building

set -e

STATIC_DIR="$(dirname "$0")/static"
mkdir -p "$STATIC_DIR"

echo "Downloading HTMX..."
curl -L -o "$STATIC_DIR/htmx.min.js" \
    "https://unpkg.com/htmx.org@2.0.4/dist/htmx.min.js"

echo "Downloading Leaflet JS..."
curl -L -o "$STATIC_DIR/leaflet.js" \
    "https://unpkg.com/leaflet@1.9.4/dist/leaflet.js"

echo "Downloading Leaflet CSS..."
curl -L -o "$STATIC_DIR/leaflet.css" \
    "https://unpkg.com/leaflet@1.9.4/dist/leaflet.css"

# Leaflet needs its images directory for markers
echo "Downloading Leaflet marker images..."
mkdir -p "$STATIC_DIR/images"
curl -L -o "$STATIC_DIR/images/marker-icon.png" \
    "https://unpkg.com/leaflet@1.9.4/dist/images/marker-icon.png"
curl -L -o "$STATIC_DIR/images/marker-icon-2x.png" \
    "https://unpkg.com/leaflet@1.9.4/dist/images/marker-icon-2x.png"
curl -L -o "$STATIC_DIR/images/marker-shadow.png" \
    "https://unpkg.com/leaflet@1.9.4/dist/images/marker-shadow.png"

echo ""
echo "Static assets ready. Now build with:"
echo "  cargo build --release"
echo "  ./target/release/skywatch"
echo ""
echo "Dashboard will be at http://localhost:3005"
