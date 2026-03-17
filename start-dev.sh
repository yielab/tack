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
echo "📍 Access points:"
echo "   🌐 API Server:    http://localhost:3210"
echo "   🌐 Health check:  http://localhost:3210/api/health"
echo ""

# Check if flexpm.local is in /etc/hosts
if grep -q "flexpm.local" /etc/hosts 2>/dev/null; then
    echo "   🌐 Custom domain: https://flexpm.local (via Caddy)"
    echo "   🌐 Custom domain: http://flexpm.local (redirects to HTTPS)"
else
    echo "   ℹ️  To use https://flexpm.local, add to /etc/hosts:"
    echo "      sudo sh -c 'echo \"127.0.0.1 flexpm.local\" >> /etc/hosts'"
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
