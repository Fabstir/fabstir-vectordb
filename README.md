# Fabstir AI Vector Database

A high-performance, decentralized vector database built on S5 storage with hybrid HNSW/IVF indexing for AI applications.

## Features

- 🚀 **High Performance**: Sub-50ms search latency with 30-second timeout for S5 operations
- 🌐 **Decentralized Storage**: Built on S5 network via Enhanced S5.js
- 🔍 **Hybrid Indexing**: HNSW for recent data, IVF for historical data
- 📊 **Scalable Architecture**: HAMT sharding activates at 1000+ vectors
- 🔧 **Multiple Interfaces**: REST API on port 7533
- 🐳 **Docker Ready**: Production and development containers

## Quick Start

### Production Deployment (After Reboot)

```bash
# 1. Start Enhanced S5.js
cd ~/dev/Fabstir/partners/S5/GitHub/s5.js/
./start-real-s5.sh &

# 2. Start Vector DB
cd ~/dev/Fabstir/fabstir-vectordb/
docker-compose -f docker-compose.dev.yml up -d vector-db-prod

# 3. Verify
curl http://localhost:5522/health         # Enhanced S5.js
curl http://localhost:7533/api/v1/health  # Vector DB
```

### Development Environment

```bash
# Start dev container with mounted workspace
docker-compose -f docker-compose.dev.yml up -d vector-db-dev

# Enter container for development
docker exec -it fabstir-vectordb-dev bash
cd /workspace  # Your code is mounted here
```

## Configuration

### Critical Settings

- **Timeout**: 30 seconds for S5 operations (src/storage/enhanced_s5_storage.rs line 51)
  - Must be `unwrap_or(30000)` not `unwrap_or(5000)`
- **Vector Dimensions**: 384 (for all-MiniLM-L6-v2)
- **Production Port**: 7533
- **Dev Ports**: 7530-7532

### Environment Variables

```bash
# S5 Configuration
S5_MODE=real                               # Use real S5 network
S5_PORTAL_URL=http://host.docker.internal:5522  # Enhanced S5.js endpoint
VECTOR_DIMENSION=384                       # Vector dimensions

# Server Configuration  
VECTOR_DB_PORT=7533                        # Server port
VECTOR_DB_HOST=0.0.0.0                    # Bind to all interfaces
```

## Architecture

```
Vector DB (port 7533)
    ↓ [HTTP PUT with 30s timeout]
Enhanced S5.js (port 5522)
    ↓ [S5 protocol]
Real S5 Network (s5.vup.cx)
    ↓ [Distributed storage]
Permanent decentralized storage
```

## API Examples

### Insert Vector
```bash
curl -X POST http://localhost:7533/api/v1/vectors \
  -H "Content-Type: application/json" \
  -d '{
    "id": "test-001",
    "vector": [0.1, 0.2, 0.3],
    "metadata": {"type": "test"}
  }'
```

### Search Vectors
```bash
curl -X POST http://localhost:7533/api/v1/search \
  -H "Content-Type: application/json" \
  -d '{
    "vector": [0.1, 0.2, 0.3],
    "k": 5
  }'
```

## Docker Containers

### Production Container (Lightweight ~100MB)
- **Image**: fabstir-vectordb-prod:latest
- **Port**: 7533
- **File**: Dockerfile.production

### Development Container  
- **Image**: fabstir-vectordb-fabstir-ai-vector-db
- **Workspace**: /workspace (mounted from host)
- **File**: docker-compose.dev.yml

## Troubleshooting

### Timeout Errors
```bash
# Check timeout setting
grep "unwrap_or" src/storage/enhanced_s5_storage.rs
# Should show: unwrap_or(30000)
```

### Port Conflicts
```bash
# Kill process on port
fuser -k 7533/tcp

# Check what's using port
lsof -i :7533
```

### Container Not Connecting to S5
```bash
# Test from inside container
docker exec vector-db-prod curl http://host.docker.internal:5522/health
```

## Development

### Building from Source
```bash
# Build release binary
cargo build --release --bin server

# Build Docker image
docker build -f Dockerfile.production -t fabstir-vectordb-prod:latest .
```

### Running Tests
```bash
# Unit tests
cargo test

# Integration tests with real S5
STORAGE_MODE=real cargo test --ignored
```

## Lessons Learned

1. **Timeout Critical**: Real S5 operations take 5-10 seconds, not milliseconds
2. **Container Networking**: Use `host.docker.internal` for container→host communication
3. **Workspace Mount**: Dev container must mount to `/workspace` for Claude Code
4. **Port Configuration**: Production uses 7533, dev uses 7530-7532

## Related Projects

- [Enhanced S5.js](../partners/S5/GitHub/s5.js/) - Storage backend
- [Fabstir LLM Marketplace](../fabstir-llm-marketplace/) - Main application

## License

MIT

## Support

For issues or questions, please open an issue on GitHub or check the [deployment guide](docs/DEPLOYMENT.md).
