#!/bin/bash
set -e

echo "🚀 Tack Local Domain Setup"
echo "================================"
echo ""

# Check if running as root for /etc/hosts modification
if [ "$EUID" -eq 0 ]; then
    SUDO=""
else
    SUDO="sudo"
fi

# Add tack.local to /etc/hosts if not already present
if ! grep -q "tack.local" /etc/hosts; then
    echo "📝 Adding tack.local to /etc/hosts..."
    echo "127.0.0.1 tack.local" | $SUDO tee -a /etc/hosts > /dev/null
    echo "✅ Added tack.local to /etc/hosts"
else
    echo "✅ tack.local already in /etc/hosts"
fi

echo ""
echo "🐳 Starting Docker containers..."
docker compose up -d

echo ""
echo "⏳ Waiting for services to be ready..."
sleep 5

# Wait for health check
echo "🔍 Checking service health..."
MAX_RETRIES=30
RETRY_COUNT=0

while [ $RETRY_COUNT -lt $MAX_RETRIES ]; do
    if curl -s http://localhost:3210/api/health > /dev/null 2>&1; then
        echo "✅ Tack API is healthy!"
        break
    fi
    RETRY_COUNT=$((RETRY_COUNT + 1))
    echo "   Waiting... ($RETRY_COUNT/$MAX_RETRIES)"
    sleep 2
done

if [ $RETRY_COUNT -eq $MAX_RETRIES ]; then
    echo "❌ Service health check timed out"
    echo "   Check logs with: docker compose logs -f"
    exit 1
fi

echo ""
echo "✨ Setup complete! Tack is now running at:"
echo ""
echo "   🌐 Direct Backend API:  http://localhost:3210"
echo "   🌐 Direct Frontend:     http://localhost:8080"
echo ""
echo "   🔒 Via Caddy (HTTPS):   https://tack.local:9443 (self-signed cert)"
echo "   🌐 Via Caddy (HTTP):    http://tack.local:9000"
echo "   🌐 Via Caddy (IP):      http://localhost:9000"
echo ""
echo "   💡 Caddy uses ports 9000/9443 to avoid conflicts with other apps"
echo ""
echo "📊 Useful commands:"
echo "   docker compose logs -f              # View all logs"
echo "   docker compose logs -f tack       # View API logs only"
echo "   docker compose exec tack tack-cli --help  # Use CLI"
echo "   curl http://localhost:3210/api/health         # Health check"
echo ""
echo "🛑 To stop: docker compose down"
echo "🗑️  To stop and remove data: docker compose down -v"
echo ""
