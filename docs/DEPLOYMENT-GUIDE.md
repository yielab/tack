# FlexPM Deployment Guide

This guide covers deploying FlexPM to production environments.

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
cd flexpm

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
- Data: Persisted in Docker volume `flexpm-data`

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
git clone <repo-url> /opt/flexpm
cd /opt/flexpm

# Create production environment file
cat > .env.production <<EOF
FLEXPM_HOST=0.0.0.0
FLEXPM_PORT=3210
FLEXPM_LOG_LEVEL=warn
FLEXPM_LOG_JSON=true
FLEXPM_LOG_FILE=/data/logs/flexpm.log
FLEXPM_STORAGE_DIR=/data/storage
EOF
```

**3. Update docker-compose.yml for production:**

```yaml
version: '3.8'

services:
  flexpm:
    build: .
    container_name: flexpm
    restart: unless-stopped
    ports:
      - "3210:3210"
    volumes:
      - flexpm-data:/data
    environment:
      - FLEXPM_HOST=0.0.0.0
      - FLEXPM_PORT=3210
      - FLEXPM_LOG_LEVEL=warn
      - FLEXPM_LOG_JSON=true
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:3210/api/health"]
      interval: 30s
      timeout: 10s
      retries: 3
    networks:
      - flexpm-network

  frontend:
    build:
      context: ./frontend
      dockerfile: Dockerfile
    container_name: flexpm-frontend
    restart: unless-stopped
    ports:
      - "8080:80"
    depends_on:
      - flexpm
    networks:
      - flexpm-network

  caddy:
    image: caddy:2-alpine
    container_name: flexpm-caddy
    restart: unless-stopped
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile
      - caddy-data:/data
      - caddy-config:/config
    depends_on:
      - flexpm
      - frontend
    networks:
      - flexpm-network

volumes:
  flexpm-data:
    driver: local
  caddy-data:
    driver: local
  caddy-config:
    driver: local

networks:
  flexpm-network:
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
cd /opt/flexpm
cargo build --release

# Build frontend
cd frontend
npm ci
npm run build
```

### Option 3: Single binary (embedded SPA)

For the simplest deployment, embed the built SPA into the API binary so one
process serves both the API (`/api/*`) and the UI (same-origin — no CORS, no
separate static host). From a clean checkout, one command does everything:

```bash
make build-spa
# → builds frontend/dist, then:
#   cargo build -p flexpm-api --release --features embed-spa
# Produces: target/release/flexpm-api  (~5 MB)
```

Run it anywhere — it needs only the SQLite database path:

```bash
FLEXPM_DATABASE_URL="sqlite:/var/lib/flexpm/flexpm.db?mode=rwc" \
  ./target/release/flexpm-api
# Open http://<host>:3210/  → the SPA loads and talks to /api same-origin.
```

The frontend client defaults to a **relative** `/api` base
(`.env.production` → `VITE_API_URL=/api`), which is what makes same-origin work.
CI builds the SPA once and the `embed-spa` job consumes that `dist/` to compile
and test the feature-gated binary (`.github/workflows/ci.yml`).

**2. Install as systemd service:**

```bash
# Create systemd service file
sudo tee /etc/systemd/system/flexpm.service <<EOF
[Unit]
Description=FlexPM Project Management API
After=network.target

[Service]
Type=simple
User=flexpm
Group=flexpm
WorkingDirectory=/opt/flexpm
Environment="FLEXPM_HOST=127.0.0.1"
Environment="FLEXPM_PORT=3210"
Environment="FLEXPM_DATABASE_URL=sqlite:/var/lib/flexpm/flexpm.db?mode=rwc"
Environment="FLEXPM_LOG_LEVEL=warn"
Environment="FLEXPM_LOG_FILE=/var/log/flexpm/flexpm.log"
Environment="FLEXPM_STORAGE_DIR=/var/lib/flexpm/storage"
ExecStart=/opt/flexpm/target/release/flexpm-api
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
EOF

# Create user and directories
sudo useradd -r -s /bin/false flexpm
sudo mkdir -p /var/lib/flexpm/storage
sudo mkdir -p /var/log/flexpm
sudo chown -R flexpm:flexpm /var/lib/flexpm /var/log/flexpm

# Enable and start
sudo systemctl daemon-reload
sudo systemctl enable flexpm
sudo systemctl start flexpm
sudo systemctl status flexpm
```

**3. Configure nginx for frontend:**

```bash
sudo tee /etc/nginx/sites-available/flexpm <<EOF
server {
    listen 80;
    server_name flexpm.example.com;

    # Frontend
    location / {
        root /opt/flexpm/frontend/dist;
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

sudo ln -s /etc/nginx/sites-available/flexpm /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl reload nginx
```

---

## Environment Configuration

### Production Environment Variables

```bash
# Server Configuration
FLEXPM_HOST=0.0.0.0                              # Bind to all interfaces
FLEXPM_PORT=3210                                 # API port

# Database
FLEXPM_DATABASE_URL=sqlite:/data/flexpm.db?mode=rwc

# Logging
FLEXPM_LOG_LEVEL=warn                            # Reduce verbosity
FLEXPM_LOG_JSON=true                             # Structured logging
FLEXPM_LOG_FILE=/data/logs/flexpm.log           # Persistent logs

# Storage
FLEXPM_STORAGE_DIR=/data/storage                 # Attachments directory

# Optional: CORS (restrict origins in production)
# FLEXPM_CORS_ORIGIN=https://flexpm.example.com
```

### Configuration File (flexpm.toml)

```toml
# Production configuration
host = "0.0.0.0"
port = 3210
database_url = "sqlite:/data/flexpm.db?mode=rwc"
log_level = "warn"
log_json = true
log_file = "/data/logs/flexpm.log"
storage_dir = "/data/storage"

# Optional: Set CORS origin
# cors_origin = "https://flexpm.example.com"
```

---

## HTTPS Setup

### Option 1: Caddy (Automatic HTTPS)

**1. Create Caddyfile:**

```caddyfile
flexpm.example.com {
    # Frontend
    handle /* {
        reverse_proxy frontend:80
    }

    # API
    handle /api/* {
        reverse_proxy flexpm:3210
    }

    # WebSocket
    @websocket {
        path /api/projects/*/board/live
    }
    handle @websocket {
        reverse_proxy flexpm:3210 {
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
        output file /var/log/caddy/flexpm.log
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
sudo certbot --nginx -d flexpm.example.com

# Auto-renewal is configured automatically
sudo systemctl status certbot.timer
```

---

## Database Backups

### Manual Backup

```bash
# Using Docker
docker compose exec flexpm sqlite3 /data/flexpm.db ".backup /data/backup-$(date +%Y%m%d).db"
docker cp flexpm:/data/backup-20260316.db ./backups/

# Using native installation
sqlite3 /var/lib/flexpm/flexpm.db ".backup /var/backups/flexpm-$(date +%Y%m%d).db"
```

### Automated Backups (cron)

```bash
# Create backup script
cat > /opt/flexpm/backup.sh <<'EOF'
#!/bin/bash
BACKUP_DIR="/var/backups/flexpm"
DATE=$(date +%Y%m%d-%H%M%S)
RETENTION_DAYS=30

mkdir -p $BACKUP_DIR

# Backup database
docker compose exec -T flexpm sqlite3 /data/flexpm.db ".backup /data/backup-$DATE.db"
docker cp flexpm:/data/backup-$DATE.db $BACKUP_DIR/
docker compose exec flexpm rm /data/backup-$DATE.db

# Backup attachments
tar -czf $BACKUP_DIR/storage-$DATE.tar.gz -C /var/lib/docker/volumes/flexpm-data/_data storage

# Delete old backups
find $BACKUP_DIR -name "*.db" -mtime +$RETENTION_DAYS -delete
find $BACKUP_DIR -name "*.tar.gz" -mtime +$RETENTION_DAYS -delete

echo "Backup completed: $DATE"
EOF

chmod +x /opt/flexpm/backup.sh

# Add to crontab (daily at 2 AM)
(crontab -l 2>/dev/null; echo "0 2 * * * /opt/flexpm/backup.sh >> /var/log/flexpm-backup.log 2>&1") | crontab -
```

### Restore from Backup

```bash
# Stop the service
docker compose stop flexpm

# Restore database
docker cp backups/backup-20260316.db flexpm:/data/flexpm.db

# Restart
docker compose start flexpm
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
docker compose logs --since 24h > flexpm-logs-$(date +%Y%m%d).log
```

**Using journalctl (systemd):**

```bash
# View logs
sudo journalctl -u flexpm -f

# Export logs
sudo journalctl -u flexpm --since "24 hours ago" > flexpm-logs.txt
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
- https://flexpm.example.com (Frontend)
- https://flexpm.example.com/api/health (Backend)

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
  flexpm:
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
// In flexpm-db initialization
sqlx::query("PRAGMA journal_mode=WAL").execute(&pool).await?;
sqlx::query("PRAGMA synchronous=NORMAL").execute(&pool).await?;
```

---

## Security Checklist

### Pre-Production

- [ ] Change default ports (optional)
- [ ] Enable HTTPS (Caddy or Certbot)
- [ ] Set `FLEXPM_LOG_LEVEL=warn` (reduce verbosity)
- [ ] Enable `FLEXPM_LOG_JSON=true` for log parsing
- [ ] Disable debug endpoints in production
- [ ] Set strict CORS origin (`FLEXPM_CORS_ORIGIN`)
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
flexpm.example.com {
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
        reverse_proxy flexpm:3210
    }
}
```

### Database Encryption

**Encrypt SQLite database:**

```bash
# Using SQLCipher
FLEXPM_DATABASE_URL="sqlite:/data/flexpm.db?cipher=sqlcipher&key=your-encryption-key"
```

Note: Requires recompiling with SQLCipher support.

---

## Troubleshooting

### Service Won't Start

**Check logs:**

```bash
docker compose logs flexpm
# or
sudo journalctl -u flexpm -n 50
```

**Common issues:**
- Port already in use → Change `FLEXPM_PORT`
- Database locked → Check for multiple instances
- Migrations failed → Check `_migrations` table

### High Memory Usage

**Check memory:**

```bash
docker stats flexpm
```

**Solutions:**
- Increase Docker memory limit
- Enable swap
- Optimize database queries

### Database Corruption

**Verify integrity:**

```bash
sqlite3 flexpm.db "PRAGMA integrity_check;"
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
/opt/flexpm/backup.sh

# 2. Pull latest code
cd /opt/flexpm
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
docker compose exec flexpm sqlite3 /data/flexpm.db "VACUUM;"
```

Run monthly or when database size is large.

### Log Rotation

```bash
# Create logrotate config
sudo tee /etc/logrotate.d/flexpm <<EOF
/var/log/flexpm/*.log {
    daily
    rotate 14
    compress
    delaycompress
    notifempty
    create 0644 flexpm flexpm
    sharedscripts
    postrotate
        systemctl reload flexpm
    endscript
}
EOF
```

---

## Production Checklist

### Before Go-Live

- [ ] DNS configured (A record for flexpm.example.com)
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
- [GitHub Issues](https://github.com/yourusername/flexpm/issues)
- [Documentation](../README.md)
- [API Reference](./API-REFERENCE.md)

**Community:**
- Discord: (coming soon)
- Forum: (coming soon)
