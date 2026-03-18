# FlexPM Access Guide

This document provides all access methods for FlexPM in your local multi-app development environment.

## 🚀 Quick Start

```bash
# Start all services
docker compose up -d

# Or use the setup script (adds flexpm.local to /etc/hosts)
./setup-local-domain.sh
```

## 📍 Access Points

FlexPM provides multiple ways to access the application, designed to work in environments with multiple applications running on different ports.

### Via Caddy Reverse Proxy (Recommended)

**HTTP (Port 9000):**
- http://localhost:9000
- http://flexpm.local:9000 *(requires /etc/hosts entry)*

**HTTPS (Port 9443 - Self-signed cert):**
- https://localhost:9443
- https://flexpm.local:9443 *(requires /etc/hosts entry)*

**Advantages:**
- ✅ Single unified endpoint for API + Frontend
- ✅ WebSocket support
- ✅ CORS headers configured
- ✅ Production-like routing
- ✅ Custom domain support

### Direct Access (No Proxy)

**Backend API (Port 3210):**
- http://localhost:3210
- http://localhost:3210/api/health
- http://localhost:3210/api/projects

**Frontend (Port 8080):**
- http://localhost:8080

**Advantages:**
- ✅ Direct access without proxy
- ✅ Useful for debugging
- ✅ Faster response times

## 🌐 Setting Up Custom Domain

To use `flexpm.local` instead of `localhost`:

### 1. Add to /etc/hosts

```bash
# Add this line to /etc/hosts
echo "127.0.0.1 flexpm.local" | sudo tee -a /etc/hosts
```

### 2. Access via custom domain

```bash
# HTTP
http://flexpm.local:9000

# HTTPS (self-signed certificate)
https://flexpm.local:9443
```

### 3. Accept self-signed certificate

When accessing HTTPS for the first time, your browser will warn about the self-signed certificate. This is expected for local development:

1. Click "Advanced"
2. Click "Proceed to flexpm.local" (or similar)
3. Certificate will be remembered for this session

## 🔧 Port Configuration

FlexPM uses non-standard ports to avoid conflicts with other applications:

| Service | Port | Protocol | Purpose |
|---------|------|----------|---------|
| **Caddy HTTP** | 9000 | HTTP | Unified proxy access |
| **Caddy HTTPS** | 9443 | HTTPS | Secure proxy access (self-signed) |
| **Backend API** | 3210 | HTTP | Direct API access |
| **Frontend** | 8080 | HTTP | Direct frontend access |

**Why these ports?**
- Ports 80/443 are often used by system services or other apps
- Ports 9000/9443 are high enough to avoid common conflicts
- All services can run alongside other development apps

## 📡 API Endpoints

### Health Check
```bash
curl http://localhost:9000/api/health
# or
curl http://localhost:3210/api/health
```

### List Projects
```bash
curl http://localhost:9000/api/projects
```

### Create Project
```bash
curl -X POST http://localhost:9000/api/projects \
  -H "Content-Type: application/json" \
  -d '{
    "name": "My Project",
    "description": "Test project",
    "project_type": "software"
  }'
```

### WebSocket (Real-time updates)
```javascript
// Connect to board live updates
const ws = new WebSocket('ws://localhost:9000/api/projects/{project_id}/board/live');

ws.onmessage = (event) => {
  const data = JSON.parse(event.data);
  console.log('Board update:', data);
};
```

## 🔍 Service Status

### Check All Services
```bash
docker compose ps
```

Expected output:
```
NAME              STATUS
flexpm            Up (healthy)
flexpm-frontend   Up (healthy)
flexpm-caddy      Up (healthy)
```

### View Logs
```bash
# All services
docker compose logs -f

# Specific service
docker compose logs -f flexpm
docker compose logs -f caddy
docker compose logs -f frontend
```

### Restart Services
```bash
# Restart all
docker compose restart

# Restart specific service
docker compose restart caddy
```

## 🛠️ Troubleshooting

### Port Already in Use

If ports 9000 or 9443 are in use, you can change them in `docker-compose.yml`:

```yaml
caddy:
  ports:
    - "9500:9000"    # Change to 9500 externally
    - "9943:9443"    # Change to 9943 externally
```

Then access via:
- http://localhost:9500
- https://localhost:9943

### Caddy Not Starting

```bash
# Check Caddy logs
docker compose logs caddy

# Validate Caddyfile
docker compose exec caddy caddy validate --config /etc/caddy/Caddyfile

# Restart Caddy
docker compose restart caddy
```

### Frontend Shows 502 Bad Gateway

```bash
# Check if backend is healthy
curl http://localhost:3210/api/health

# Restart backend
docker compose restart flexpm

# Wait for health check
docker compose logs -f flexpm | grep "listening"
```

### Can't Access via flexpm.local

```bash
# Verify /etc/hosts entry
grep flexpm.local /etc/hosts

# Should output:
# 127.0.0.1 flexpm.local

# If not present, add it:
echo "127.0.0.1 flexpm.local" | sudo tee -a /etc/hosts
```

## 🚦 Testing Your Setup

### Quick Test Script
```bash
#!/bin/bash

echo "Testing FlexPM Access..."
echo ""

echo "1. Backend API (direct):"
curl -s http://localhost:3210/api/health && echo " ✅ OK" || echo " ❌ FAIL"

echo "2. Frontend (direct):"
curl -s -I http://localhost:8080 | head -1 && echo " ✅ OK" || echo " ❌ FAIL"

echo "3. Caddy HTTP:"
curl -s http://localhost:9000/api/health && echo " ✅ OK" || echo " ❌ FAIL"

echo "4. Caddy HTTPS:"
curl -sk https://localhost:9443/api/health && echo " ✅ OK" || echo " ❌ FAIL"

echo ""
echo "All tests complete!"
```

### Browser Test

1. Open http://localhost:9000
2. You should see the FlexPM frontend
3. Open browser DevTools (F12) → Network tab
4. Refresh the page
5. Verify requests go to `/api/*` endpoints

## 📊 Performance Notes

### Response Times

Using Caddy adds minimal overhead:

- **Direct API:** ~5ms
- **Via Caddy:** ~6-8ms (+1-3ms proxy overhead)

For development, this overhead is negligible.

### WebSocket Performance

WebSockets through Caddy work transparently:
- ✅ Automatic protocol upgrade
- ✅ Connection keep-alive
- ✅ Low latency (<10ms additional)

## 🔐 Security Notes

### For Local Development

- Self-signed certificates are OK for local development
- CORS is wide open (`Access-Control-Allow-Origin: *`)
- No authentication required

### For Production

Before deploying to production:

1. **Get real SSL certificates**
   - Use Let's Encrypt
   - Caddy can auto-provision with `tls your@email.com`

2. **Restrict CORS**
   - Update Caddyfile to specific origins
   - Remove wildcard `*`

3. **Add authentication**
   - Implement JWT or session-based auth
   - Add middleware to protect routes

4. **Use standard ports**
   - Change to ports 80/443
   - Update Caddyfile and docker-compose.yml

## 📚 See Also

- [README.md](README.md) - Project overview
- [DEPLOYMENT-GUIDE.md](docs/DEPLOYMENT-GUIDE.md) - Production deployment
- [API-REFERENCE.md](docs/API-REFERENCE.md) - Complete API documentation
- [TESTING.md](docs/TESTING.md) - Testing guide

## 🆘 Getting Help

If you encounter issues:

1. Check service status: `docker compose ps`
2. View logs: `docker compose logs`
3. Verify ports: `docker compose ps` (check PORTS column)
4. Test health: `curl http://localhost:3210/api/health`
5. Restart: `docker compose restart`

For persistent issues:
```bash
# Full reset (WARNING: deletes data)
docker compose down -v
docker compose up -d --build
```
