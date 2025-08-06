# Vector DB S5 Storage Integration

## Overview
The Vector Database now supports configurable S5 storage backends through environment variables, enabling both mock and real S5 portal connections.

## Configuration

### Environment Variables

| Variable | Description | Default | Example |
|----------|-------------|---------|---------|
| `STORAGE_MODE` | Storage backend mode | `mock` | `mock` or `real` |
| `S5_MOCK_SERVER_URL` | S5 backend URL for mock mode | `http://localhost:5522` | `http://s5-server:5522` |
| `S5_NODE_URL` | S5 portal URL for real mode | `https://s5.vup.cx` | `https://s5.vup.cx` |
| `DATABASE_URL` | PostgreSQL connection | Required | `postgresql://user:pass@host:5432/db` |
| `RUST_LOG` | Logging level | `info` | `debug`, `info`, `warn`, `error` |

## Storage Modes

### Mock Mode (`STORAGE_MODE=mock`)
- Uses simplified storage interface
- Connects to Enhanced S5.js server or mock S5 service
- Suitable for development and testing
- Lower latency, higher throughput

### Real Mode (`STORAGE_MODE=real`)
- Connects directly to S5 portal
- Full distributed storage capabilities
- Production deployment
- Higher latency, distributed resilience

## API Changes

### Health Endpoint
The `/api/v1/health` endpoint now shows actual storage configuration:

```json
{
  "status": "healthy",
  "version": "0.1.0",
  "storage": {
    "mode": "mock",
    "connected": true,
    "base_url": "http://s5-server:5522"
  },
  "indices": {
    "hnsw": { "healthy": true, "vector_count": 0 },
    "ivf": { "healthy": true, "vector_count": 0 }
  }
}
```

## Storage Paths

Vectors are stored in S5 with the following path structure:
- `/s5/fs/vectors/{vector_id}` - Vector data and metadata
- `/s5/fs/indices/{index_type}` - Index structures
- `/s5/fs/metadata/{vector_id}` - Additional metadata

## Implementation Details

### Files Modified
- `src/api/rest.rs` - Removed hardcoded port, added env var support
- `src/storage/s5_storage_factory.rs` - Enhanced storage factory with mode selection

### Key Changes
1. **Dynamic Configuration**: All S5 URLs now configurable via environment
2. **Mode Selection**: Support for mock/real backend switching
3. **Health Reporting**: Accurate storage status in health endpoint
4. **Fallback Behavior**: Sensible defaults when env vars not set

## Docker Deployment

```yaml
services:
  vector-db:
    image: fabstir-vectordb:latest
    environment:
      - STORAGE_MODE=mock
      - S5_MOCK_SERVER_URL=http://s5-server:5522
      - DATABASE_URL=postgresql://postgres:postgres@db:5432/vectordb
      - RUST_LOG=info
    ports:
      - "8080:8080"
```

## Testing

```bash
# Test with mock S5
export STORAGE_MODE=mock
export S5_MOCK_SERVER_URL="http://localhost:5522"
./target/release/server

# Test with real S5
export STORAGE_MODE=real
export S5_NODE_URL="https://s5.vup.cx"
./target/release/server
```

## Performance Metrics

| Operation | Mock Mode | Real Mode |
|-----------|-----------|-----------|
| Vector Insert | <10ms | 50-100ms |
| Vector Search | <50ms | 100-200ms |
| Bulk Insert (1K) | 0.58s | 5-10s |
| Bulk Insert (10K) | 5.37s | 50-100s |

## Migration Path

1. Start with `STORAGE_MODE=mock` for development
2. Test with Enhanced S5.js server
3. Switch to `STORAGE_MODE=real` for production
4. No data migration needed (same storage format)

## Related Projects
- Enhanced S5.js: Provides storage backend
- Fabstir LLM Node: Main integration project (Phase 4.3.1)

---
Created: August 2025
Version: 1.0.0
