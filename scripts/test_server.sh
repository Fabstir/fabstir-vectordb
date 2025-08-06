#!/bin/bash

# Test script for vector-db server with proper S5 configuration

echo "=== Vector DB Server Configuration Test ==="
echo

# Set environment variables
export STORAGE_MODE=mock
export S5_MOCK_SERVER_URL="http://s5-server:5522"
export VECTOR_DB_PORT=7530
export RUST_LOG=info

echo "Environment Configuration:"
echo "  STORAGE_MODE=$STORAGE_MODE"
echo "  S5_MOCK_SERVER_URL=$S5_MOCK_SERVER_URL"
echo "  VECTOR_DB_PORT=$VECTOR_DB_PORT"
echo

# Kill any existing server
pkill -f "target/release/server" 2>/dev/null

# Start the server in the background
echo "Starting server..."
./target/release/server &
SERVER_PID=$!

# Wait for server to start
sleep 3

# Check if server is running
if ps -p $SERVER_PID > /dev/null; then
    echo "✓ Server started successfully (PID: $SERVER_PID)"
    
    # Test the health endpoint
    echo
    echo "Testing health endpoint..."
    HEALTH_RESPONSE=$(curl -s http://localhost:7530/api/v1/health)
    
    if [ $? -eq 0 ]; then
        echo "✓ Health endpoint responded"
        echo "Response:"
        echo "$HEALTH_RESPONSE" | jq . 2>/dev/null || echo "$HEALTH_RESPONSE"
        
        # Check if the response contains the correct URL
        if echo "$HEALTH_RESPONSE" | grep -q "s5-server:5522"; then
            echo
            echo "✅ SUCCESS: Server is using the correct S5_MOCK_SERVER_URL!"
        else
            echo
            echo "⚠️  WARNING: Server might not be using the correct URL"
        fi
    else
        echo "✗ Failed to reach health endpoint"
    fi
    
    # Kill the server
    kill $SERVER_PID 2>/dev/null
    echo
    echo "Server stopped."
else
    echo "✗ Server failed to start"
    echo "Check the logs for errors"
fi

# Clean up
rm -f test_config test_config.rs 2>/dev/null