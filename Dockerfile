# ─── Stage 1: Build ─────────────────────────────────
FROM rust:1.86-slim AS builder

WORKDIR /app

# Install build dependencies for SQLite
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY crates/flexpm-core/Cargo.toml crates/flexpm-core/Cargo.toml
COPY crates/flexpm-db/Cargo.toml crates/flexpm-db/Cargo.toml
COPY crates/flexpm-api/Cargo.toml crates/flexpm-api/Cargo.toml
COPY crates/flexpm-cli/Cargo.toml crates/flexpm-cli/Cargo.toml

# Create dummy source files so cargo can resolve the workspace and cache deps
RUN mkdir -p crates/flexpm-core/src && echo "pub fn _dummy() {}" > crates/flexpm-core/src/lib.rs \
    && mkdir -p crates/flexpm-db/src && echo "pub fn _dummy() {}" > crates/flexpm-db/src/lib.rs \
    && mkdir -p crates/flexpm-api/src && echo "fn main() {}" > crates/flexpm-api/src/main.rs \
    && echo "pub fn _dummy() {}" > crates/flexpm-api/src/lib.rs \
    && mkdir -p crates/flexpm-cli/src && echo "fn main() {}" > crates/flexpm-cli/src/main.rs

# Build dependencies only (cached unless Cargo.toml/lock changes)
RUN cargo build --release 2>/dev/null || true

# Copy actual source code
COPY crates/ crates/

# Touch source files so cargo knows they changed (not the cached dummies)
RUN find crates -name "*.rs" -exec touch {} +

# Build the real binaries
RUN cargo build --release --bin flexpm-api --bin flexpm-cli

# ─── Stage 2: Runtime ───────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    sqlite3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd -r flexpm && useradd -r -g flexpm -m flexpm

# Create data directories
RUN mkdir -p /data /data/storage && chown -R flexpm:flexpm /data

WORKDIR /data

# Copy binaries from builder
COPY --from=builder /app/target/release/flexpm-api /usr/local/bin/flexpm-api
COPY --from=builder /app/target/release/flexpm-cli /usr/local/bin/flexpm-cli

# Copy example config
COPY config/flexpm.example.toml /etc/flexpm/flexpm.example.toml

USER flexpm

# Default environment for Docker (bind 0.0.0.0, store DB in /data)
ENV FLEXPM_HOST=0.0.0.0 \
    FLEXPM_PORT=3210 \
    FLEXPM_DATABASE_URL=sqlite:/data/flexpm.db?mode=rwc \
    FLEXPM_STORAGE_DIR=/data/storage \
    FLEXPM_LOG_LEVEL=info

EXPOSE 3210

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3210/api/health || exit 1

ENTRYPOINT ["flexpm-api"]
