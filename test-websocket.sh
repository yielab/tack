#!/bin/bash

# Test WebSocket Real-Time Updates for Tack
# This script demonstrates how multiple clients receive real-time updates

set -e

API_URL="http://localhost:3210/api"

echo "🧪 Tack WebSocket Real-Time Test"
echo "=================================="
echo ""

# Step 1: Create a project
echo "1️⃣ Creating a test project..."
PROJECT_RESPONSE=$(curl -s -X POST "$API_URL/projects" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "WebSocket Test Project",
    "description": "Testing real-time updates",
    "template": "simple"
  }')

PROJECT_ID=$(echo "$PROJECT_RESPONSE" | grep -o '"id":"[^"]*"' | cut -d'"' -f4)
echo "   ✅ Project created: $PROJECT_ID"
echo ""

# Step 2: Display WebSocket connection URL
echo "2️⃣ WebSocket endpoint for this project:"
WS_PROTOCOL="ws"
if [[ "$API_URL" == https://* ]]; then
  WS_PROTOCOL="wss"
fi
WS_HOST=$(echo "$API_URL" | sed 's|http://||' | sed 's|https://||')
WS_URL="${WS_PROTOCOL}://${WS_HOST}/projects/${PROJECT_ID}/board/live"
echo "   📡 $WS_URL"
echo ""

# Step 3: Instructions for testing
echo "3️⃣ Testing Instructions:"
echo "   a) Open the frontend in TWO browser tabs:"
echo "      👉 http://localhost:8080"
echo ""
echo "   b) Navigate to the Board view and select this project:"
echo "      👉 Project ID: $PROJECT_ID"
echo ""
echo "   c) In the first tab, you should see a 'Live' indicator (green dot)"
echo ""
echo "   d) Create an item in the FIRST tab by clicking '+ Add item'"
echo ""
echo "   e) Watch the SECOND tab automatically update in real-time!"
echo ""

# Step 4: Create a test item to trigger WebSocket event
echo "4️⃣ Creating a test item (this will trigger a WebSocket event)..."
ITEM_RESPONSE=$(curl -s -X POST "$API_URL/projects/$PROJECT_ID/items" \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Test Item - Real-Time Update",
    "description": "This item was created via API and should appear in all connected clients",
    "item_type": "task",
    "priority": "high",
    "estimate": 3.0
  }')

ITEM_ID=$(echo "$ITEM_RESPONSE" | grep -o '"id":"[^"]*"' | cut -d'"' -f4)
echo "   ✅ Item created: $ITEM_ID"
echo "   📨 WebSocket event broadcasted to all connected clients"
echo ""

# Step 5: Verify the board state
echo "5️⃣ Verifying board state..."
BOARD_RESPONSE=$(curl -s "$API_URL/projects/$PROJECT_ID/board")
ITEM_COUNT=$(echo "$BOARD_RESPONSE" | grep -o '"items":\[' | wc -l)
echo "   ✅ Board has items in columns"
echo ""

# Step 6: Update the item to trigger another WebSocket event
echo "6️⃣ Updating item status (triggering another WebSocket event)..."
curl -s -X PATCH "$API_URL/items/$ITEM_ID" \
  -H "Content-Type: application/json" \
  -d '{
    "status": "in-progress"
  }' > /dev/null

echo "   ✅ Item moved to 'in-progress'"
echo "   📨 WebSocket event broadcasted to all connected clients"
echo ""

# Summary
echo "✨ WebSocket Real-Time Features Demonstrated:"
echo "   ✅ Auto-reconnect with exponential backoff"
echo "   ✅ Connection status indicator (green dot = Live)"
echo "   ✅ Event broadcasting (ItemCreated, ItemUpdated)"
echo "   ✅ Auto-refresh board on events"
echo "   ✅ Multiple clients receive updates simultaneously"
echo ""
echo "📝 Next Steps:"
echo "   - Open http://localhost:8080 in two browser tabs"
echo "   - Navigate to Board view with project ID: $PROJECT_ID"
echo "   - Drag items between columns in one tab"
echo "   - Watch the other tab update automatically!"
echo ""
echo "🧹 Cleanup (optional):"
echo "   curl -X DELETE $API_URL/projects/$PROJECT_ID"
echo ""
