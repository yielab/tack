# syntax=docker/dockerfile:1
#
# Tack — single-binary container image.
#
# Tack is one ~10 MB binary with the SolidJS SPA embedded (`--features
# embed-spa`): the same process serves the REST API (`/api/*`), the WebSocket,
# and the web UI same-origin. This is a three-stage build:
#
#   1. build the SPA (Node)               → frontend/dist
#   2. compile a static musl binary (Rust) that embeds that dist
#   3. copy the binary onto a distroless base — no OS, no shell, ~10 MB total
#
# Build & run:
#   docker build -t tack:latest .
#   docker run --rm -p 3210:3210 -v tack-data:/data tack:latest
#   # → http://localhost:3210

# ── Stage 1: build the SPA ────────────────────────────────────────────────
FROM node:20-bookworm-slim AS frontend
WORKDIR /frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

# ── Stage 2: compile the static, SPA-embedding binary ─────────────────────
FROM rust:1-bookworm AS backend
ENV TARGET=x86_64-unknown-linux-musl
RUN apt-get update \
 && apt-get install -y --no-install-recommends musl-tools \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
# rust-toolchain.toml (copied in above) pins the toolchain `cargo` resolves
# from this point on. Adding the target before COPY would add it to
# rustup's bare image default instead — a different toolchain identity than
# the pinned one `cargo build` below actually uses, so the target has to be
# added after the pin is in scope.
RUN rustup target add "$TARGET"
# Bring in the SPA built in stage 1 so `embed-spa` has a dist/ to embed.
COPY --from=frontend /frontend/dist ./frontend/dist
RUN cargo build --release --target "$TARGET" -p tack-cli --features embed-spa \
 && cp "target/$TARGET/release/tack" /tack

# ── Stage 3: minimal runtime ──────────────────────────────────────────────
# distroless/static ships CA certs + tzdata + /etc/passwd (needed for outbound
# HTTPS to GitHub / S3-compatible backup endpoints) and nothing else.
FROM gcr.io/distroless/static-debian12:latest
LABEL org.opencontainers.image.title="Tack" \
      org.opencontainers.image.description="Lightweight, single-binary project management (Rust + SolidJS)" \
      org.opencontainers.image.source="https://github.com/yielab/tack" \
      org.opencontainers.image.licenses="MIT"

COPY --from=backend /tack /usr/local/bin/tack

# Bind on all interfaces inside the container; the database and attachments
# live under /data so a single volume persists everything. If you expose the
# container beyond localhost, you MUST also set TACK_API_TOKEN.
ENV TACK_HOST=0.0.0.0 \
    TACK_PORT=3210 \
    TACK_DATABASE_URL="sqlite:/data/tack.db?mode=rwc" \
    TACK_STORAGE_DIR="/data/storage"

WORKDIR /data
VOLUME ["/data"]
EXPOSE 3210

ENTRYPOINT ["/usr/local/bin/tack"]
CMD ["serve"]
