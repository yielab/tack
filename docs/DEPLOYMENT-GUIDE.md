# Tack Deployment Guide

This guide covers deploying Tack to production environments.

---

## Table of Contents

1. [Quick Deploy (Docker)](#quick-deploy-docker)
2. [Production Deployment](#production-deployment)
3. [Environment Configuration](#environment-configuration)
4. [HTTPS Setup](#https-setup)
5. [Database Backups](#database-backups)
6. [Monitoring & Logging](#monitoring--logging)
7. [Scaling Considerations](#scaling-considerations)
8. [Security Checklist](#security-checklist)
9. [Troubleshooting](#troubleshooting)

---

## Quick Deploy (Docker)

**For development/testing:**

```bash
# Clone the repository
git clone <repo-url>
cd tack

# Start all services
docker compose up -d

# Verify deployment
curl http://localhost:3210/api/health
curl http://localhost:8080

# View logs
docker compose logs -f
```

**Services:**
- Backend API: http://localhost:3210
- Frontend: http://localhost:8080
- Data: Persisted in Docker volume `tack-data`

---

## Production Deployment

### Option 1: Docker Compose (Recommended)

**1. Prepare the server:**

```bash
# Install Docker and Docker Compose
curl -fsSL https://get.docker.com | sh
sudo usermod -aG docker $USER
```

**2. Clone and configure:**

```bash
git clone <repo-url> /opt/tack
cd /opt/tack

# Create production environment file
cat > .env.production <<EOF
TACK_HOST=0.0.0.0
TACK_PORT=3210
TACK_LOG_LEVEL=warn
TACK_LOG_JSON=true
TACK_LOG_FILE=/data/logs/tack.log
TACK_STORAGE_DIR=/data/storage
EOF
```

**3. Update docker-compose.yml for production:**

```yaml
version: '3.8'

services:
  tack:
    build: .
    container_name: tack
    restart: unless-stopped
    ports:
      - "3210:3210"
    volumes:
      - tack-data:/data
    environment:
      - TACK_HOST=0.0.0.0
      - TACK_PORT=3210
      - TACK_LOG_LEVEL=warn
      - TACK_LOG_JSON=true
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:3210/api/health"]
      interval: 30s
      timeout: 10s
      retries: 3
    networks:
      - tack-network

  frontend:
    build:
      context: ./frontend
      dockerfile: Dockerfile
    container_name: tack-frontend
    restart: unless-stopped
    ports:
      - "8080:80"
    depends_on:
      - tack
    networks:
      - tack-network

  caddy:
    image: caddy:2-alpine
    container_name: tack-caddy
    restart: unless-stopped
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile
      - caddy-data:/data
      - caddy-config:/config
    depends_on:
      - tack
      - frontend
    networks:
      - tack-network

volumes:
  tack-data:
    driver: local
  caddy-data:
    driver: local
  caddy-config:
    driver: local

networks:
  tack-network:
    driver: bridge
```

**4. Deploy:**

```bash
# Build and start
docker compose -f docker-compose.yml up -d --build

# Verify
docker compose ps
docker compose logs -f
```

### Option 2: Native Deployment

**1. Build from source:**

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build backend
cd /opt/tack
cargo build --release

# Build frontend
cd frontend
npm ci
npm run build
```

### Option 3: Single binary (embedded SPA)

For the simplest deployment, embed the built SPA into the single `tack` binary
so one process serves both the API (`/api/*`) and the UI (same-origin — no CORS,
no separate static host). From a clean checkout, one command does everything:

```bash
make build
# → builds frontend/dist, then:
#   cargo build -p tack-cli --release --features embed-spa
# Produces the single binary: target/release/tack  (~10 MB)
```

Run it anywhere — it needs only the SQLite database path:

```bash
TACK_DATABASE_URL="sqlite:/var/lib/tack/tack.db?mode=rwc" \
  ./target/release/tack
# Open http://<host>:3210/  → the SPA loads and talks to /api same-origin.
```

The frontend client defaults to a **relative** `/api` base
(`.env.production` → `VITE_API_URL=/api`), which is what makes same-origin work.
CI builds the SPA once and the `embed-spa` job consumes that `dist/` to compile
and test the feature-gated binary (`.github/workflows/ci.yml`).

**2. Install as systemd service:**

```bash
# Create systemd service file
sudo tee /etc/systemd/system/tack.service <<EOF
[Unit]
Description=Tack Project Management API
After=network.target

[Service]
Type=simple
User=tack
Group=tack
WorkingDirectory=/opt/tack
Environment="TACK_HOST=127.0.0.1"
Environment="TACK_PORT=3210"
Environment="TACK_DATABASE_URL=sqlite:/var/lib/tack/tack.db?mode=rwc"
Environment="TACK_LOG_LEVEL=warn"
Environment="TACK_LOG_FILE=/var/log/tack/tack.log"
Environment="TACK_STORAGE_DIR=/var/lib/tack/storage"
ExecStart=/opt/tack/target/release/tack
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
EOF

# Create user and directories
sudo useradd -r -s /bin/false tack
sudo mkdir -p /var/lib/tack/storage
sudo mkdir -p /var/log/tack
sudo chown -R tack:tack /var/lib/tack /var/log/tack

# Enable and start
sudo systemctl daemon-reload
sudo systemctl enable tack
sudo systemctl start tack
sudo systemctl status tack
```

**3. Configure nginx for frontend:**

```bash
sudo tee /etc/nginx/sites-available/tack <<EOF
server {
    listen 80;
    server_name tack.example.com;

    # Frontend
    location / {
        root /opt/tack/frontend/dist;
        try_files \$uri \$uri/ /index.html;
        expires 1h;
        add_header Cache-Control "public, immutable";
    }

    # API proxy
    location /api/ {
        proxy_pass http://127.0.0.1:3210;
        proxy_http_version 1.1;
        proxy_set_header Upgrade \$http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;

        # WebSocket support
        proxy_read_timeout 86400;
    }
}
EOF

sudo ln -s /etc/nginx/sites-available/tack /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl reload nginx
```

---

## Environment Configuration

### Production Environment Variables

```bash
# Server Configuration
TACK_HOST=0.0.0.0                              # Bind to all interfaces
TACK_PORT=3210                                 # API port

# Database
TACK_DATABASE_URL=sqlite:/data/tack.db?mode=rwc

# Logging
TACK_LOG_LEVEL=warn                            # Reduce verbosity
TACK_LOG_JSON=true                             # Structured logging
TACK_LOG_FILE=/data/logs/tack.log           # Persistent logs

# Storage
TACK_STORAGE_DIR=/data/storage                 # Attachments directory

# Optional: CORS (restrict origins in production)
# TACK_CORS_ORIGIN=https://tack.example.com
```

### Configuration File (tack.toml)

```toml
# Production configuration
host = "0.0.0.0"
port = 3210
database_url = "sqlite:/data/tack.db?mode=rwc"
log_level = "warn"
log_json = true
log_file = "/data/logs/tack.log"
storage_dir = "/data/storage"

# Optional: Set CORS origin
# cors_origin = "https://tack.example.com"
```

---

## HTTPS Setup

### Option 1: Caddy (Automatic HTTPS)

**1. Create Caddyfile:**

```caddyfile
tack.example.com {
    # Frontend
    handle /* {
        reverse_proxy frontend:80
    }

    # API
    handle /api/* {
        reverse_proxy tack:3210
    }

    # WebSocket
    @websocket {
        path /api/projects/*/board/live
    }
    handle @websocket {
        reverse_proxy tack:3210 {
            header_up Upgrade {http.request.header.Upgrade}
            header_up Connection {http.request.header.Connection}
        }
    }

    # Security headers
    header {
        Strict-Transport-Security "max-age=31536000; includeSubDomains; preload"
        X-Content-Type-Options "nosniff"
        X-Frame-Options "SAMEORIGIN"
        X-XSS-Protection "1; mode=block"
    }

    # Gzip compression
    encode gzip

    # Logging
    log {
        output file /var/log/caddy/tack.log
        level INFO
    }
}
```

**2. Deploy with Caddy:**

```bash
docker compose up -d caddy
```

Caddy automatically obtains and renews Let's Encrypt certificates.

### Option 2: nginx + Certbot

```bash
# Install Certbot
sudo apt install certbot python3-certbot-nginx

# Obtain certificate
sudo certbot --nginx -d tack.example.com

# Auto-renewal is configured automatically
sudo systemctl status certbot.timer
```

---

## Database Backups

### Manual Backup

```bash
# Using Docker
docker compose exec tack sqlite3 /data/tack.db ".backup /data/backup-$(date +%Y%m%d).db"
docker cp tack:/data/backup-20260316.db ./backups/

# Using native installation
sqlite3 /var/lib/tack/tack.db ".backup /var/backups/tack-$(date +%Y%m%d).db"
```

### Automated Backups (cron)

```bash
# Create backup script
cat > /opt/tack/backup.sh <<'EOF'
#!/bin/bash
BACKUP_DIR="/var/backups/tack"
DATE=$(date +%Y%m%d-%H%M%S)
RETENTION_DAYS=30

mkdir -p $BACKUP_DIR

# Backup database
docker compose exec -T tack sqlite3 /data/tack.db ".backup /data/backup-$DATE.db"
docker cp tack:/data/backup-$DATE.db $BACKUP_DIR/
docker compose exec tack rm /data/backup-$DATE.db

# Backup attachments
tar -czf $BACKUP_DIR/storage-$DATE.tar.gz -C /var/lib/docker/volumes/tack-data/_data storage

# Delete old backups
find $BACKUP_DIR -name "*.db" -mtime +$RETENTION_DAYS -delete
find $BACKUP_DIR -name "*.tar.gz" -mtime +$RETENTION_DAYS -delete

echo "Backup completed: $DATE"
EOF

chmod +x /opt/tack/backup.sh

# Add to crontab (daily at 2 AM)
(crontab -l 2>/dev/null; echo "0 2 * * * /opt/tack/backup.sh >> /var/log/tack-backup.log 2>&1") | crontab -
```

### Restore from Backup

```bash
# Stop the service
docker compose stop tack

# Restore database
docker cp backups/backup-20260316.db tack:/data/tack.db

# Restart
docker compose start tack
```

---

## Monitoring & Logging

### Health Check Endpoint

```bash
# Check API health
curl http://localhost:3210/api/health

# Expected response:
# {"status":"healthy","version":"1.0.0","uptime_seconds":86400}
```

### Log Aggregation

**Using Docker logs:**

```bash
# View real-time logs
docker compose logs -f

# Export logs
docker compose logs --since 24h > tack-logs-$(date +%Y%m%d).log
```

**Using journalctl (systemd):**

```bash
# View logs
sudo journalctl -u tack -f

# Export logs
sudo journalctl -u tack --since "24 hours ago" > tack-logs.txt
```

### Application Metrics

**Debug endpoints (development only):**

```bash
# System info
curl http://localhost:3210/api/debug/info

# Database stats
curl http://localhost:3210/api/debug/db-stats
```

**Disable in production:** Comment out debug routes in `router.rs`.

### External Monitoring

**Example: Uptime Kuma**

```bash
# Monitor endpoints
- https://tack.example.com (Frontend)
- https://tack.example.com/api/health (Backend)

# Alert conditions
- HTTP status != 200
- Response time > 5s
- Downtime > 2 minutes
```

---

## Scaling Considerations

### Vertical Scaling

**Increase resources:**

```yaml
# docker-compose.yml
services:
  tack:
    deploy:
      resources:
        limits:
          cpus: '2.0'
          memory: 2G
        reservations:
          cpus: '1.0'
          memory: 512M
```

### Horizontal Scaling

**Limitations:**
- SQLite is single-writer (no horizontal scaling for writes)
- Read replicas possible with WAL mode

**For high-scale deployments:**
1. Migrate to PostgreSQL
2. Add read replicas
3. Use Redis for session storage
4. Add load balancer

### Database Optimization

**Enable WAL mode:**

```sql
-- Faster writes, better concurrency
PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;
PRAGMA cache_size=-64000;  -- 64MB cache
PRAGMA temp_store=memory;
```

**Add to startup:**

```rust
// In tack-db initialization
sqlx::query("PRAGMA journal_mode=WAL").execute(&pool).await?;
sqlx::query("PRAGMA synchronous=NORMAL").execute(&pool).await?;
```

---

## Security Checklist

### Pre-Production

- [ ] Change default ports (optional)
- [ ] Enable HTTPS (Caddy or Certbot)
- [ ] Set `TACK_LOG_LEVEL=warn` (reduce verbosity)
- [ ] Enable `TACK_LOG_JSON=true` for log parsing
- [ ] Disable debug endpoints in production
- [ ] Set strict CORS origin (`TACK_CORS_ORIGIN`)
- [ ] Configure firewall (allow 80, 443; block 3210, 8080)
- [ ] Use strong database file permissions
- [ ] Enable log rotation
- [ ] Set up automated backups
- [ ] Configure rate limiting (reverse proxy)

### Firewall Configuration

```bash
# Using UFW (Ubuntu)
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp
sudo ufw deny 3210/tcp  # Block direct API access
sudo ufw deny 8080/tcp  # Block direct frontend access
sudo ufw enable
```

### Rate Limiting (Caddy)

```caddyfile
tack.example.com {
    # Rate limit API requests
    rate_limit {
        zone api {
            key {remote_host}
            events 100
            window 1m
        }
    }

    # Apply to API routes
    @api path /api/*
    handle @api {
        rate_limit api
        reverse_proxy tack:3210
    }
}
```

### Database Encryption

**Encrypt SQLite database:**

```bash
# Using SQLCipher
TACK_DATABASE_URL="sqlite:/data/tack.db?cipher=sqlcipher&key=your-encryption-key"
```

Note: Requires recompiling with SQLCipher support.

---

## Troubleshooting

### Service Won't Start

**Check logs:**

```bash
docker compose logs tack
# or
sudo journalctl -u tack -n 50
```

**Common issues:**
- Port already in use → Change `TACK_PORT`
- Database locked → Check for multiple instances
- Migrations failed → Check `_migrations` table

### High Memory Usage

**Check memory:**

```bash
docker stats tack
```

**Solutions:**
- Increase Docker memory limit
- Enable swap
- Optimize database queries

### Database Corruption

**Verify integrity:**

```bash
sqlite3 tack.db "PRAGMA integrity_check;"
```

**Recovery:**
1. Stop service
2. Restore from backup
3. Run integrity check
4. Restart service

### WebSocket Connection Issues

**Check nginx/Caddy config:**
- Ensure `Upgrade` and `Connection` headers are proxied
- Increase `proxy_read_timeout` to 86400s (24 hours)
- Verify firewall allows WebSocket connections

**Test WebSocket:**

```bash
# Using websocat
websocat ws://localhost:3210/api/projects/<project-id>/board/live
```

### Performance Issues

**Profile the database:**

```sql
EXPLAIN QUERY PLAN SELECT * FROM items WHERE project_id = ?;
```

**Add indexes:**

```sql
CREATE INDEX idx_items_status ON items(status);
CREATE INDEX idx_items_priority ON items(priority);
```

---

## Maintenance

### Update Procedure

```bash
# 1. Backup
/opt/tack/backup.sh

# 2. Pull latest code
cd /opt/tack
git pull

# 3. Rebuild and restart
docker compose up -d --build

# 4. Verify
curl http://localhost:3210/api/health
docker compose logs -f
```

### Database Vacuum

```bash
# Reclaim space and optimize
docker compose exec tack sqlite3 /data/tack.db "VACUUM;"
```

Run monthly or when database size is large.

### Log Rotation

```bash
# Create logrotate config
sudo tee /etc/logrotate.d/tack <<EOF
/var/log/tack/*.log {
    daily
    rotate 14
    compress
    delaycompress
    notifempty
    create 0644 tack tack
    sharedscripts
    postrotate
        systemctl reload tack
    endscript
}
EOF
```

---

## Production Checklist

### Before Go-Live

- [ ] DNS configured (A record for tack.example.com)
- [ ] HTTPS enabled and tested
- [ ] Backups automated and tested (restore test)
- [ ] Monitoring configured (health checks)
- [ ] Log aggregation configured
- [ ] Security checklist completed
- [ ] Performance tested (load testing)
- [ ] Documentation reviewed
- [ ] Team trained on basic operations

### After Go-Live

- [ ] Monitor logs for errors (first 24 hours)
- [ ] Verify backups running successfully
- [ ] Check disk space
- [ ] Review performance metrics
- [ ] Collect user feedback
- [ ] Document any issues

---

## Support

**Resources:**
- [GitHub Issues](https://github.com/yielab/tack/issues)
- [Documentation](../README.md)
- [API Reference](./API-REFERENCE.md)

**Community:**
- Discord: (coming soon)
- Forum: (coming soon)
