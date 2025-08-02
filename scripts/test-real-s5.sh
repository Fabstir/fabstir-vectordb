#!/bin/bash

# Script to run real S5 integration tests

echo "Starting Real S5 Integration Tests..."
echo "======================================="
echo ""

# Check if Enhanced s5.js npm package is available
if ! npm list @parajbs-dev/s5client-js &>/dev/null; then
    echo "Warning: @parajbs-dev/s5client-js not found."
    echo "You may need to install it or use a different package name."
    echo ""
fi

# Option 1: Run locally (requires Node.js and Enhanced s5.js installed)
if [ "$1" == "local" ]; then
    echo "Running tests locally..."
    
    # Start the real S5 server in background
    cd scripts
    npm install
    S5_SEED_PHRASE="${S5_SEED_PHRASE}" node real-s5-server.js &
    SERVER_PID=$!
    cd ..
    
    # Wait for server to start
    echo "Waiting for server to start..."
    sleep 5
    
    # Run tests
    cargo test test_s5_real_integration -- --ignored --nocapture
    TEST_EXIT_CODE=$?
    
    # Stop server
    kill $SERVER_PID
    
    exit $TEST_EXIT_CODE
fi

# Option 2: Run with Docker (default)
echo "Running tests with Docker..."
echo ""

# Build and run with docker-compose
docker-compose -f docker-compose.real-s5.yml up --build --abort-on-container-exit

# Clean up
docker-compose -f docker-compose.real-s5.yml down

echo ""
echo "Real S5 Integration Tests Complete!"