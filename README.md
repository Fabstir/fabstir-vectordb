# Fabstir AI Vector Database

A high-performance, decentralized vector database built on S5 storage with hybrid HNSW/IVF indexing for AI applications. Optimized for video metadata search with support for 10M+ vectors and sub-50ms search latency.

## Features

- 🚀 **High Performance**: Sub-50ms search latency (p99) with support for 10M+ vectors
- 🌐 **Decentralized Storage**: Built on S5 network for immutable, content-addressed storage
- 🔍 **Hybrid Indexing**: HNSW for recent data, IVF for historical data with automatic migration
- 📊 **Scalable Architecture**: HAMT sharding activates at 1000+ vectors for O(log n) lookups
- 🔧 **Multiple Interfaces**: REST API, MCP server for LLM integration, WebSocket/SSE streaming
- 🛡️ **Secure Configuration**: Enhanced seed phrase management with file support and validation
- 🐳 **Docker Ready**: Full Docker and Docker Compose support for easy deployment

## Quick Start

### Using Docker (Recommended)

```bash
# Clone the repository
git clone https://github.com/fabstir/fabstir-ai-vector-db.git
cd fabstir-ai-vector-db

# Start with Docker Compose
docker-compose up --build

# The services will be available at:
# - REST API: http://localhost:7530
# - MCP Server: http://localhost:7531
# - Admin Interface: http://localhost:7532
```

### Native Installation

```bash
# Prerequisites: Rust 1.70+ and Node.js 18+

# Clone and build
git clone https://github.com/fabstir/fabstir-ai-vector-db.git
cd fabstir-ai-vector-db

# Build the Rust backend
cargo build --release

# Run the server
cargo run --bin server
```

## Configuration

### Environment Variables

Create a `.env` file or set environment variables:

```bash
# S5 Storage Configuration
S5_MODE=mock                             # Storage mode: "mock" or "real" (default: mock)
S5_PORTAL_URL=https://s5.vup.cx          # S5 network portal (required for real mode)
S5_MOCK_SERVER_URL=http://localhost:5524 # Mock server URL (required for mock mode)
S5_SEED_PHRASE_FILE=~/.s5-seed           # Path to seed phrase file (recommended)
S5_CONNECTION_TIMEOUT=5000               # Connection timeout in ms
S5_RETRY_ATTEMPTS=3                      # Number of retry attempts

# Vector Database Configuration
VECTOR_DIMENSION=1536                    # Vector dimensions (default: 1536 for OpenAI)
HAMT_ACTIVATION_THRESHOLD=1000           # Vectors count to activate HAMT sharding

# Index Configuration
HNSW_M=16                               # HNSW connectivity parameter
HNSW_EF_CONSTRUCTION=200                # HNSW construction quality
IVF_N_CLUSTERS=256                      # IVF number of clusters
```

### Seed Phrase Setup

For production use with S5 network:

```bash
# Create seed phrase file with secure permissions
echo "your twelve word seed phrase goes here like this example phrase" > ~/.s5-seed
chmod 600 ~/.s5-seed

# Configure environment
export S5_MODE=real
export S5_PORTAL_URL=https://s5.vup.cx
export S5_SEED_PHRASE_FILE=~/.s5-seed
```

## API Usage

### Basic Example

```typescript
import { VectorDBClient } from 'fabstir-ai-vector-db';

const client = new VectorDBClient({
  apiUrl: 'http://localhost:7530'
});

// Insert a vector
const result = await client.insertVector({
  id: 'vec_001',
  vector: [0.1, 0.2, 0.3, ...], // 1536-dimensional vector
  metadata: {
    video_id: 'video_123',
    title: 'Introduction to AI',
    tags: ['ai', 'tutorial']
  }
});

// Search for similar vectors
const searchResults = await client.search({
  vector: queryVector,
  k: 10,
  options: {
    include_metadata: true,
    score_threshold: 0.8
  }
});
```

### Health Check

```bash
curl http://localhost:7530/health
```

Response:
```json
{
  "status": "healthy",
  "version": "0.1.0",
  "storage": {
    "mode": "mock",
    "connected": true,
    "base_url": "http://localhost:5524"
  },
  "indices": {
    "hnsw": { "healthy": true, "vector_count": 1234 },
    "ivf": { "healthy": true, "vector_count": 5678 }
  }
}
```

## Architecture

The system uses a hybrid architecture optimized for both recent and historical data:

```
┌─────────────────────┐     ┌─────────────────────┐
│   REST API Layer    │     │   MCP Server Layer  │
│   (Port: 7530)      │     │   (Port: 7531)      │
└──────────┬──────────┘     └──────────┬──────────┘
           │                           │
           └───────────┬───────────────┘
                       │
            ┌──────────▼──────────┐
            │   Hybrid Index      │
            │  ┌─────────────┐    │
            │  │ HNSW Index  │    │  ← Recent vectors (fast updates)
            │  │ (Recent)    │    │
            │  └─────────────┘    │
            │  ┌─────────────┐    │
            │  │ IVF Index   │    │  ← Historical vectors (space efficient)
            │  │ (Historical)│    │
            │  └─────────────┘    │
            └──────────┬──────────┘
                       │
            ┌──────────▼──────────┐
            │ Enhanced S5 Storage │  ← Mock/Real modes
            │   Adapter Layer     │
            └─────────────────────┘
```

## Performance

### Benchmarks

On standard hardware (16 CPU cores, 32GB RAM):

| Operation | Vectors | Latency (p50) | Latency (p99) | Throughput |
|-----------|---------|---------------|---------------|------------|
| Insert    | 1M      | 0.5ms         | 2ms           | 2000 ops/s |
| Search    | 1M      | 15ms          | 45ms          | 1000 QPS   |
| Search    | 10M     | 25ms          | 50ms          | 500 QPS    |

### Memory Usage

- HNSW Index: ~500 bytes per vector
- IVF Index: ~100 bytes per vector
- 10M vectors: ~10GB RAM

## Development

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test module
cargo test test_configuration_management -- --test-threads=1

# Run with verbose output
cargo test -- --nocapture
```

### Building Documentation

```bash
# Generate Rust documentation
cargo doc --open

# Build JavaScript bindings
cd bindings/js
npm install
npm run build
```

### Code Quality

```bash
# Format code
cargo fmt

# Run linter
cargo clippy

# Type checking
cargo check
```

## MCP Server Integration

The MCP (Model Context Protocol) server enables direct LLM integration:

```json
{
  "mcpServers": {
    "fabstir-vector-db": {
      "command": "node",
      "args": ["./mcp-server.js"],
      "env": {
        "VECTOR_DB_URL": "http://localhost:7530",
        "MCP_PORT": "7531"
      }
    }
  }
}
```

## Docker Deployment

### Docker Compose Configuration

```yaml
version: "3.8"

services:
  fabstir-ai-vector-db:
    build: .
    ports:
      - "7530:7530"  # REST API
      - "7531:7531"  # MCP Server
      - "7532:7532"  # Admin Interface
    environment:
      - S5_MODE=${S5_MODE:-mock}
      - S5_PORTAL_URL=${S5_PORTAL_URL:-https://s5.vup.cx}
    volumes:
      - vector-data:/home/developer/fabstir-ai-vector-db/data
      - ${S5_SEED_PHRASE_FILE:-/dev/null}:/app/seed.txt:ro

volumes:
  vector-data:
```

### Production Deployment

For production deployments:

1. Use environment-specific `.env` files
2. Enable HTTPS with proper certificates
3. Configure rate limiting and authentication
4. Set up monitoring and alerting
5. Use persistent volumes for data

## Troubleshooting

### Common Issues

1. **Configuration Errors**
   ```bash
   # Error: S5_PORTAL_URL required for real mode
   export S5_MODE=real
   export S5_PORTAL_URL=https://s5.vup.cx
   ```

2. **Seed Phrase Validation**
   ```bash
   # Error: Invalid seed phrase: expected 12 or 24 words, got 10
   # Ensure your seed phrase has exactly 12 or 24 words
   ```

3. **Connection Issues**
   ```bash
   # Increase timeout for slow connections
   export S5_CONNECTION_TIMEOUT=10000
   export S5_RETRY_ATTEMPTS=5
   ```

### Debug Mode

Enable debug logging:

```bash
export RUST_LOG=vector_db=debug,tower_http=debug
cargo run --bin server
```

## Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

### Development Setup

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Run tests (`cargo test`)
5. Commit your changes (`git commit -m 'Add amazing feature'`)
6. Push to the branch (`git push origin feature/amazing-feature`)
7. Open a Pull Request

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Support

- 📚 [Full API Documentation](docs/API.md)
- 🐛 [Issue Tracker](https://github.com/fabstir/fabstir-ai-vector-db/issues)
- 💬 [Discord Community](https://discord.gg/fabstir)
- 📧 [Email Support](mailto:support@fabstir.ai)

## Acknowledgments

- Built on [S5 Network](https://s5.cx) for decentralized storage
- Uses [HNSW algorithm](https://arxiv.org/abs/1603.09320) for approximate nearest neighbor search
- Implements [IVF indexing](https://github.com/facebookresearch/faiss) for scalable similarity search

---

Made with ❤️ by the Fabstir team