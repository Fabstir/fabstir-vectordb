#!/bin/bash
# start-fabstir-vectordb-prod.sh - Lightweight Vector DB Production Container

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}🚀 Starting Fabstir Vector DB Production Container${NC}"
echo "=================================="

# Check if S5.js is running
if ! curl -s http://localhost:5522/health > /dev/null 2>&1; then
    echo -e "${RED}❌ S5.js is not running on port 5522${NC}"
    echo "  Start it first: cd ~/dev/Fabstir/partners/S5/GitHub/s5.js && ./start-s5js-prod.sh"
    exit 1
fi
echo -e "${GREEN}✅ S5.js is running on port 5522${NC}"

# Configuration - Always use real S5.js
S5_URL="http://host.docker.internal:5522"
S5_MODE="real"  # Always real when using production S5.js

# Cleanup old containers
# More thorough cleanup
echo -e "${YELLOW}🧹 Cleaning up old Vector DB containers...${NC}"

# Stop and remove vectordb-prod specifically
docker stop vectordb-prod 2>/dev/null || true
docker rm vectordb-prod 2>/dev/null || true

# Find and stop ANY container on port 7533
CONTAINERS_ON_7533=$(docker ps --format "{{.Names}}" --filter "publish=7533" || true)
if [ ! -z "$CONTAINERS_ON_7533" ]; then
    echo "  Found containers on port 7533: $CONTAINERS_ON_7533"
    for container in $CONTAINERS_ON_7533; do
        echo "  Stopping $container..."
        docker stop $container 2>/dev/null || true
        docker rm $container 2>/dev/null || true
    done
fi

# Kill any non-Docker process on port
fuser -k 7533/tcp 2>/dev/null || true
sleep 2

# Check timeout configuration
echo -e "${YELLOW}Checking timeout configuration...${NC}"
if grep -q "unwrap_or(30000)" src/storage/enhanced_s5_storage.rs; then
    echo -e "${GREEN}✓ Timeout correctly set to 30 seconds${NC}"
else
    echo -e "${RED}WARNING: Timeout not set to 30000ms!${NC}"
    echo -e "${YELLOW}Fixing timeout...${NC}"
    sed -i 's/unwrap_or([0-9]*)/unwrap_or(30000)/' src/storage/enhanced_s5_storage.rs
    cargo build --release --bin server
fi

# Check if production image exists
if ! docker images | grep -q "fabstir-vectordb-prod"; then
    echo -e "${YELLOW}Building Vector DB production image...${NC}"
    if [ -f "Dockerfile.production" ]; then
        docker build -f Dockerfile.production -t fabstir-vectordb-prod:latest .
    else
        echo -e "${RED}❌ Dockerfile.production not found${NC}"
        echo "  Please create it or run from docker-compose.dev.yml"
        exit 1
    fi
fi

# Run container
echo -e "${YELLOW}Starting Vector DB production container...${NC}"
docker run -d \
  --name vectordb-prod \
  -p 7533:7533 \
  -e VECTOR_DB_PORT=7533 \
  -e VECTOR_DB_HOST=0.0.0.0 \
  -e S5_MODE=${S5_MODE} \
  -e S5_PORTAL_URL=${S5_URL} \
  -e STORAGE_MODE=real \
  -e VECTOR_DIMENSION=384 \
  -e RUST_LOG=info \
  --add-host host.docker.internal:host-gateway \
  --restart no \
  fabstir-vectordb-prod:latest

# Wait for startup
echo -e "${YELLOW}⏳ Waiting for Vector DB to start...${NC}"
MAX_WAIT=30
WAITED=0
while [ $WAITED -lt $MAX_WAIT ]; do
    if curl -s http://localhost:7533/api/v1/health > /dev/null 2>&1; then
        echo -e "${GREEN}✅ Vector DB is healthy!${NC}"
        break
    fi
    sleep 1
    WAITED=$((WAITED + 1))
done

if [ $WAITED -eq $MAX_WAIT ]; then
    echo -e "${RED}❌ Vector DB failed to start${NC}"
    docker logs vectordb-prod
    exit 1
fi

# Test vector insertion
echo -e "${YELLOW}Testing Vector DB...${NC}"
TEST_ID="test-$(date +%s)"
RESPONSE=$(curl -s -X POST http://localhost:7533/api/v1/vectors \
  -H "Content-Type: application/json" \
  -d "{\"id\":\"$TEST_ID\",\"vector\":[0.1,0.2,0.3],\"metadata\":{\"type\":\"startup-test\"}}")

if echo "$RESPONSE" | grep -q "success\|$TEST_ID"; then
    echo -e "${GREEN}✅ Vector insertion successful${NC}"
else
    echo -e "${YELLOW}⚠️  Vector insertion response: $RESPONSE${NC}"
fi

echo ""
echo -e "${GREEN}✅ Vector DB Production Container Started${NC}"
echo "=================================="
echo "  Container: vectordb-prod"
echo "  Port: 7533"
echo "  S5 Backend: $S5_URL"
echo "  Mode: $S5_MODE (using real S5.js)"
echo ""
echo "Commands:"
echo "  Logs: docker logs -f vectordb-prod"
echo "  Stop: docker stop vectordb-prod && docker rm vectordb-prod"
echo ""
echo "Test with:"
echo "  curl http://localhost:7533/api/v1/health"