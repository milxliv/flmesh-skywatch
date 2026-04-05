# Stage 1: Build
FROM rust:1.83-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
# Cache dependency build
RUN mkdir src && echo 'fn main() {}' > src/main.rs && cargo build --release && rm -rf src

COPY src/ src/
COPY migrations/ migrations/
RUN touch src/main.rs && cargo build --release

# Stage 2: Runtime
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/skywatch .
COPY static/ static/
COPY templates/ templates/
COPY data/ data/

# Download static assets if not present
RUN if [ ! -f static/htmx.min.js ]; then \
    apt-get update && apt-get install -y --no-install-recommends curl && \
    curl -sL -o static/htmx.min.js https://unpkg.com/htmx.org@2.0.4/dist/htmx.min.js && \
    curl -sL -o static/leaflet.js https://unpkg.com/leaflet@1.9.4/dist/leaflet.js && \
    curl -sL -o static/leaflet.css https://unpkg.com/leaflet@1.9.4/dist/leaflet.css && \
    apt-get purge -y curl && apt-get autoremove -y && rm -rf /var/lib/apt/lists/*; \
    fi

EXPOSE 3005

ENV RUST_LOG=info

CMD ["./skywatch", "--address", "0.0.0.0", "--port", "3005"]
