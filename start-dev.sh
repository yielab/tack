#!/bin/bash
set -e

echo "🚀 FlexPM Development Environment"
echo "==================================="
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
        echo "✅ FlexPM API is healthy!"
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
echo "✨ FlexPM is now running!"
echo ""
echo "📍 Access Points:"
echo ""
echo "   🌐 Via Caddy (recommended):  http://localhost:9000"
echo "   🔒 Via Caddy HTTPS:          https://localhost:9443 (self-signed cert)"
echo ""
echo "   🔧 Direct Backend API:       http://localhost:3210"
echo "   🔧 Direct Frontend:          http://localhost:8080"
echo ""
echo "   💡 Caddy uses ports 9000/9443 to avoid conflicts with other apps"
echo ""

# Check if flexpm.local is in /etc/hosts
if grep -q "flexpm.local" /etc/hosts 2>/dev/null; then
    echo "🎯 Custom Domain: ENABLED"
    echo "   🌐 http://flexpm.local:9000"
    echo "   🔒 https://flexpm.local:9443"
else
    echo "🎯 Optional: Custom Domain Setup"
    echo ""
    echo "   To use http://flexpm.local:9000 instead of localhost:"
    echo "   sudo sh -c 'echo \"127.0.0.1 flexpm.local\" >> /etc/hosts'"
    echo ""
    echo "   Note: .local domains are handled by mDNS, so /etc/hosts entry is needed"
fi

echo ""
echo "📊 Useful commands:"
echo "   docker compose logs -f              # View all logs"
echo "   docker compose logs -f flexpm       # View API logs only"
echo "   docker compose exec flexpm flexpm-cli --help  # Use CLI"
echo "   curl http://localhost:3210/api/health         # Health check"
echo ""
echo "🛑 To stop: docker compose down"
echo "🗑️  To stop and remove data: docker compose down -v"
echo ""
