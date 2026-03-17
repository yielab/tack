#!/bin/bash

# FlexPM Quick Start Script
# This script helps new users get started with FlexPM quickly

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Print colored output
print_info() { echo -e "${BLUE}ℹ${NC} $1"; }
print_success() { echo -e "${GREEN}✓${NC} $1"; }
print_warning() { echo -e "${YELLOW}⚠${NC} $1"; }
print_error() { echo -e "${RED}✗${NC} $1"; }

# Banner
echo "================================================"
echo "         FlexPM Quick Start"
echo "================================================"
echo ""

# Check prerequisites
print_info "Checking prerequisites..."

if ! command -v docker &> /dev/null; then
    print_error "Docker is not installed"
    echo "Please install Docker: https://docs.docker.com/get-docker/"
    exit 1
fi
print_success "Docker is installed"

if ! command -v docker compose &> /dev/null; then
    print_error "Docker Compose is not installed"
    echo "Please install Docker Compose: https://docs.docker.com/compose/install/"
    exit 1
fi
print_success "Docker Compose is installed"

# Check if services are already running
if docker compose ps | grep -q "Up"; then
    print_warning "FlexPM services are already running"
    echo ""
    echo "Running services:"
    docker compose ps
    echo ""
    read -p "Do you want to restart services? (y/N) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        print_info "Restarting services..."
        docker compose restart
    fi
else
    # Start services
    print_info "Starting FlexPM services..."
    docker compose up -d

    # Wait for services to be ready
    print_info "Waiting for services to be ready..."
    sleep 5

    # Check health
    for i in {1..30}; do
        if curl -s http://localhost:3210/api/health > /dev/null 2>&1; then
            break
        fi
        echo -n "."
        sleep 1
    done
    echo ""
fi

# Verify services
print_info "Verifying services..."

# Check backend
if curl -s http://localhost:3210/api/health > /dev/null 2>&1; then
    print_success "Backend API is running (http://localhost:3210)"
else
    print_error "Backend API is not responding"
    echo "Check logs: docker compose logs flexpm"
    exit 1
fi

# Check frontend
if curl -s http://localhost:8080 > /dev/null 2>&1; then
    print_success "Frontend is running (http://localhost:8080)"
else
    print_error "Frontend is not responding"
    echo "Check logs: docker compose logs frontend"
    exit 1
fi

echo ""
print_success "FlexPM is ready!"
echo ""

# Display access information
echo "================================================"
echo "         Access Information"
echo "================================================"
echo ""
echo "Frontend:  http://localhost:8080"
echo "Backend:   http://localhost:3210"
echo "Health:    http://localhost:3210/api/health"
echo ""

# Create demo data option
read -p "Do you want to create demo data? (y/N) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    print_info "Creating demo project..."

    # Create project
    PROJECT_RESPONSE=$(curl -s -X POST http://localhost:3210/api/projects \
      -H "Content-Type: application/json" \
      -d '{"name":"Demo Project","description":"A sample software project with demo data","project_type":"software"}')

    PROJECT_ID=$(echo $PROJECT_RESPONSE | python3 -c "import sys, json; print(json.load(sys.stdin)['id'])" 2>/dev/null || echo "")

    if [ -z "$PROJECT_ID" ]; then
        print_error "Failed to create demo project"
        echo "Response: $PROJECT_RESPONSE"
    else
        print_success "Demo project created: $PROJECT_ID"

        # Create demo items
        print_info "Creating demo items..."

        curl -s -X POST http://localhost:3210/api/projects/$PROJECT_ID/items \
          -H "Content-Type: application/json" \
          -d '{"title":"Design user interface","item_type":"task","status":"Done","priority":"high","estimate":8,"tags":["frontend","design"]}' > /dev/null

        curl -s -X POST http://localhost:3210/api/projects/$PROJECT_ID/items \
          -H "Content-Type: application/json" \
          -d '{"title":"Implement API endpoints","item_type":"task","status":"In Progress","priority":"high","estimate":13,"tags":["backend","api"]}' > /dev/null

        curl -s -X POST http://localhost:3210/api/projects/$PROJECT_ID/items \
          -H "Content-Type: application/json" \
          -d '{"title":"Set up CI/CD pipeline","item_type":"task","status":"To Do","priority":"medium","estimate":5,"tags":["devops","automation"]}' > /dev/null

        curl -s -X POST http://localhost:3210/api/projects/$PROJECT_ID/items \
          -H "Content-Type: application/json" \
          -d '{"title":"Write documentation","item_type":"task","status":"To Do","priority":"low","estimate":3,"tags":["docs"]}' > /dev/null

        curl -s -X POST http://localhost:3210/api/projects/$PROJECT_ID/items \
          -H "Content-Type: application/json" \
          -d '{"title":"Fix authentication bug","item_type":"bug","status":"In Review","priority":"critical","estimate":2,"tags":["backend","security","bug"]}' > /dev/null

        print_success "Demo data created (5 items)"
        echo ""
        echo "Open http://localhost:8080 to view the demo project!"
    fi
fi

echo ""
echo "================================================"
echo "         Next Steps"
echo "================================================"
echo ""
echo "1. Open http://localhost:8080 in your browser"
echo "2. Create a new project or view the demo"
echo "3. Start adding items and organizing your work!"
echo ""
echo "Useful commands:"
echo "  - View logs:     docker compose logs -f"
echo "  - Stop services: docker compose down"
echo "  - Restart:       docker compose restart"
echo ""
echo "Documentation:"
echo "  - README:        ./README.md"
echo "  - API Docs:      ./docs/API-REFERENCE.md"
echo "  - Shortcuts:     ./docs/KEYBOARD-SHORTCUTS.md"
echo ""
print_success "Happy project managing! 🚀"
echo ""
