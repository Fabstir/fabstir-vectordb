# Fabstir AI Vector Database API Documentation

## Overview

Fabstir AI Vector Database is a decentralized vector database built on top of Enhanced S5.js with HAMT sharding for efficient O(log n) lookups. It provides high-performance vector similarity search capabilities for AI applications, with a focus on video metadata search and decentralized storage.

### Key Features

- **Decentralized Storage**: Built on S5 network for immutable, content-addressed storage
- **Hybrid Indexing**: Combines HNSW (Hierarchical Navigable Small World) for recent data and IVF (Inverted File) for historical data
- **HAMT Sharding**: Automatic activation at 1000+ vectors for scalable storage
- **MCP Server Integration**: Enables direct integration with LLMs via Model Context Protocol
- **High Performance**: Supports 10M+ vectors with <50ms search latency (p99)
- **Time-based Partitioning**: Automatic migration between indices based on data age
- **CBOR Serialization**: Compatible with S5 storage format

### Architecture Overview

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
            │  │ HNSW Index  │    │
            │  │ (Recent)    │    │
            │  └─────────────┘    │
            │  ┌─────────────┐    │
            │  │ IVF Index   │    │
            │  │ (Historical)│    │
            │  └─────────────┘    │
            └──────────┬──────────┘
                       │
            ┌──────────▼──────────┐
            │   S5 Storage Layer  │
            │  ┌─────────────┐    │
            │  │Enhanced S5  │    │
            │  │ Adapter     │    │
            │  └──────┬──────┘    │
            │         │           │
            │  ┌──────▼──────┐    │
            │  │ Mock/Real   │    │
            │  │   Modes     │    │
            │  └─────────────┘    │
            │  ┌─────────────┐    │
            │  │ CBOR Format │    │
            │  └─────────────┘    │
            │  ┌─────────────┐    │
            │  │ HAMT Shard  │    │
            │  │ (>1000 vecs)│    │
            │  └─────────────┘    │
            └─────────────────────┘
```

**Phase 8 Enhancements**:
- Enhanced S5 Storage Adapter with mock and real modes
- Improved configuration management with validation
- Secure seed phrase handling (file support, validation)
- Better error messages and health monitoring

## Installation & Setup

### Prerequisites

- Docker and Docker Compose (recommended)
- OR: Rust 1.70+ and Node.js 18+ (for native installation)
- S5 Portal access (default: https://s5.vup.cx)

### Installation Steps

#### Using Docker (Recommended)

```bash
# Clone the repository
git clone https://github.com/fabstir/fabstir-ai-vector-db.git
cd fabstir-ai-vector-db

# Start with Docker Compose
docker-compose up --build

# The API will be available at:
# - REST API: http://localhost:7530
# - MCP Server: http://localhost:7531
# - Admin Interface: http://localhost:7532
```

#### Native Installation

```bash
# Clone the repository
git clone https://github.com/fabstir/fabstir-ai-vector-db.git
cd fabstir-ai-vector-db

# Build the Rust backend
cargo build --release

# Install JavaScript dependencies (for bindings)
cd bindings/js
npm install
npm run build
cd ../..

# Run the server
cargo run --bin server
```

### Configuration Options

Environment variables can be set in `.env` file or passed directly:

```bash
# S5 Storage Configuration
S5_MODE=mock                             # Storage mode: "mock" or "real" (default: mock)
S5_PORTAL_URL=https://s5.vup.cx          # S5 network portal URL (required for real mode)
S5_MOCK_SERVER_URL=http://localhost:5524 # Mock server URL (required for mock mode)
S5_SEED_PHRASE=your-seed-phrase-here     # Optional: S5 seed phrase (12 or 24 words)
S5_SEED_PHRASE_FILE=/path/to/seed.txt    # Optional: Path to seed phrase file
S5_API_KEY=your-api-key                  # Optional: S5 API key
S5_CONNECTION_TIMEOUT=5000               # Connection timeout in ms (default: 5000)
S5_RETRY_ATTEMPTS=3                      # Number of retry attempts (default: 3)

# Vector Database Configuration
VECTOR_DIMENSION=1536                     # Vector dimensions (default: 1536 for OpenAI)
MAX_VECTORS_PER_INDEX=1000000            # Maximum vectors per index
HAMT_BRANCHING_FACTOR=32                 # HAMT branching factor
HAMT_ACTIVATION_THRESHOLD=1000           # Vectors count to activate HAMT sharding

# Index Configuration
HNSW_M=16                                # HNSW connectivity parameter
HNSW_EF_CONSTRUCTION=200                 # HNSW construction quality
IVF_N_CLUSTERS=256                       # IVF number of clusters
IVF_N_PROBE=16                          # IVF clusters to search

# Server Configuration
VECTOR_DB_HOST=0.0.0.0                   # Server host (default: 0.0.0.0)
VECTOR_DB_PORT=7530                      # REST API port (default in code: 8080, recommended: 7530)
MCP_SERVER_PORT=7531                     # MCP server port
ADMIN_PORT=7532                          # Admin interface port
VECTOR_DB_MAX_REQUEST_SIZE=10485760      # Max request size (10MB)
VECTOR_DB_TIMEOUT_SECS=30                # Request timeout
VECTOR_DB_CORS_ORIGINS=http://localhost:3000  # CORS origins
```

### Docker Setup

The project includes a comprehensive Docker setup:

```yaml
# docker-compose.yml
version: "3.8"

services:
  fabstir-ai-vector-db:
    build: .
    ports:
      - "7530:7530"  # REST API
      - "7531:7531"  # MCP Server
      - "7532:7532"  # Admin Interface
    environment:
      - S5_PORTAL_URL=${S5_PORTAL_URL:-https://s5.vup.cx}
      - VECTOR_DIMENSION=${VECTOR_DIMENSION:-1536}
      - HAMT_ACTIVATION_THRESHOLD=${HAMT_ACTIVATION_THRESHOLD:-1000}
    volumes:
      - vector-data:/home/developer/fabstir-ai-vector-db/data
```

## Core API Reference

### REST API Endpoints

Base URL: `http://localhost:7530/api/v1`

#### Health Check

```http
GET /health
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
    "hnsw": {
      "healthy": true,
      "vector_count": 1234
    },
    "ivf": {
      "healthy": true,
      "vector_count": 5678
    }
  }
}
```

The `storage` object provides information about the S5 storage backend:
- `mode`: Either "mock" or "real" depending on configuration
- `connected`: Whether the storage backend is accessible
- `base_url`: Present for mock mode, shows the mock server URL
- `portal_url`: Present for real mode, shows the S5 portal URL

#### Vector Operations

##### Insert Vector

```http
POST /vectors
Content-Type: application/json

{
  "id": "vec_12345",
  "vector": [0.1, 0.2, 0.3, ...],
  "metadata": {
    "video_id": "video_abc",
    "title": "Example Video",
    "tags": ["ai", "tutorial"]
  }
}
```

Response:
```json
{
  "id": "vec_12345",
  "index": "recent",
  "timestamp": "2025-01-15T10:30:00Z"
}
```

##### Batch Insert

```http
POST /vectors/batch
Content-Type: application/json

{
  "vectors": [
    {
      "id": "vec_001",
      "vector": [0.1, 0.2, ...],
      "metadata": {...}
    },
    {
      "id": "vec_002",
      "vector": [0.3, 0.4, ...],
      "metadata": {...}
    }
  ]
}
```

Response:
```json
{
  "successful": 2,
  "failed": 0,
  "errors": []
}
```

##### Get Vector

```http
GET /vectors/{id}
```

Response:
```json
{
  "id": "vec_12345",
  "vector": [0.1, 0.2, 0.3, ...],
  "metadata": {
    "video_id": "video_abc",
    "title": "Example Video"
  },
  "index": "recent",
  "timestamp": "2025-01-15T10:30:00Z"
}
```

##### Delete Vector

```http
DELETE /vectors/{id}
```

Response: `204 No Content`

#### Search Operations

##### Vector Search

```http
POST /search
Content-Type: application/json

{
  "vector": [0.1, 0.2, 0.3, ...],
  "k": 10,
  "filter": {
    "tags": ["ai", "tutorial"]
  },
  "options": {
    "search_recent": true,
    "search_historical": true,
    "hnsw_ef": 50,
    "ivf_n_probe": 16,
    "timeout_ms": 5000,
    "include_metadata": true,
    "score_threshold": 0.8
  }
}
```

Response:
```json
{
  "results": [
    {
      "id": "vec_12345",
      "distance": 0.15,
      "score": 0.92,
      "metadata": {
        "video_id": "video_abc",
        "title": "Example Video"
      }
    }
  ],
  "search_time_ms": 23.5,
  "indices_searched": 2,
  "partial_results": false
}
```

#### Admin Operations

##### Get Statistics

```http
GET /admin/statistics
```

Response:
```json
{
  "total_vectors": 10000,
  "recent_vectors": 2000,
  "historical_vectors": 8000,
  "memory_usage": {
    "total_bytes": 104857600,
    "hnsw_bytes": 41943040,
    "ivf_bytes": 62914560
  }
}
```

##### Trigger Migration

```http
POST /admin/migrate
```

Response:
```json
{
  "vectors_migrated": 500,
  "duration_ms": 1234.5
}
```

##### Rebalance Clusters

```http
POST /admin/rebalance
```

Response:
```json
{
  "clusters_modified": 32,
  "vectors_moved": 1500
}
```

##### Create Backup

```http
POST /admin/backup
Content-Type: application/json

{
  "backup_path": "/backups/vector-db-backup.tar.gz",
  "compress": true
}
```

Response:
```json
{
  "backup_size": 52428800,
  "vectors_backed_up": 10000,
  "compression_ratio": 0.65
}
```

#### Streaming Operations

##### Server-Sent Events (SSE)

```http
GET /stream/updates
```

Event stream format:
```
event: vector_added
data: {"id": "vec_12345", "index": "recent"}

event: migration_started
data: {"vectors_to_migrate": 500}

event: migration_completed
data: {"vectors_migrated": 500, "duration_ms": 1234}
```

##### WebSocket Connection

```http
GET /ws
Upgrade: websocket
```

## MCP Server Integration

The MCP (Model Context Protocol) server enables direct integration with LLMs. It runs on port 7531 by default.

### Connecting LLMs via MCP

#### Configuration Example

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

### Available MCP Endpoints

#### Vector Search Tool

```json
{
  "name": "vector_search",
  "description": "Search for similar vectors in the database",
  "input_schema": {
    "type": "object",
    "properties": {
      "query_vector": {
        "type": "array",
        "items": {"type": "number"}
      },
      "k": {
        "type": "integer",
        "default": 10
      },
      "filter": {
        "type": "object"
      }
    },
    "required": ["query_vector"]
  }
}
```

#### Insert Vector Tool

```json
{
  "name": "insert_vector",
  "description": "Insert a new vector into the database",
  "input_schema": {
    "type": "object",
    "properties": {
      "id": {"type": "string"},
      "vector": {
        "type": "array",
        "items": {"type": "number"}
      },
      "metadata": {"type": "object"}
    },
    "required": ["id", "vector"]
  }
}
```

### Example MCP Integration

```javascript
// Example: Using with Claude or other LLM
const mcp = new MCPClient({
  url: 'http://localhost:7531',
  apiKey: process.env.MCP_API_KEY
});

// Search for similar videos
const results = await mcp.call('vector_search', {
  query_vector: embeddingVector,
  k: 5,
  filter: {
    tags: ['ai', 'tutorial']
  }
});
```

## Data Models

### Vector Format

```typescript
interface Vector {
  id: string;           // Unique identifier (e.g., "vec_12345")
  vector: number[];     // Float32 array of embeddings
  metadata?: object;    // Optional metadata
}
```

### VectorId

Vectors are identified using a 32-byte BLAKE3 hash:

```rust
pub struct VectorId([u8; 32]);

impl VectorId {
    pub fn new() -> Self;                    // Generate random ID
    pub fn from_string(s: &str) -> Self;     // Hash from string
    pub fn hash_hex(&self) -> String;        // Hex representation
    pub fn to_string(&self) -> String;       // Human-readable format
}
```

### Embedding

```rust
pub struct Embedding {
    data: Vec<f32>,
}

impl Embedding {
    pub fn dimension(&self) -> usize;
    pub fn magnitude(&self) -> f32;
    pub fn normalize(&self) -> Self;
    pub fn cosine_similarity(&self, other: &Self) -> f32;
    pub fn euclidean_distance(&self, other: &Self) -> f32;
}
```

### Video Metadata

```typescript
interface VideoMetadata {
  video_id: string;
  title: string;
  description?: string;
  tags: string[];
  duration_seconds: number;
  upload_timestamp: string;  // ISO 8601
  model_name: string;        // Embedding model used
  extra: Record<string, any>;
}
```

### Search Result

```typescript
interface SearchResult {
  id: string;
  distance: number;    // Euclidean distance
  score: number;       // Normalized similarity score (0-1)
  metadata?: object;
}
```

### Index Configuration

#### HNSW Configuration

```rust
pub struct HNSWConfig {
    pub max_connections: usize,          // M parameter (default: 16)
    pub max_connections_layer_0: usize,  // M*2 for layer 0 (default: 32)
    pub ef_construction: usize,          // Construction quality (default: 200)
    pub seed: Option<u64>,              // Random seed
}
```

#### IVF Configuration

```rust
pub struct IVFConfig {
    pub n_clusters: usize,      // Number of clusters (default: 256)
    pub n_probe: usize,         // Clusters to search (default: 16)
    pub train_size: usize,      // Training set size (default: 10000)
    pub max_iterations: usize,  // K-means iterations (default: 25)
    pub seed: Option<u64>,      // Random seed
}
```

#### Hybrid Configuration

```rust
pub struct HybridConfig {
    pub recent_threshold: Duration,      // Age threshold (default: 7 days)
    pub hnsw_config: HNSWConfig,
    pub ivf_config: IVFConfig,
    pub migration_batch_size: usize,     // Batch size for migration (default: 100)
    pub auto_migrate: bool,              // Auto-migrate old vectors (default: true)
}
```

## S5 Storage Integration

### How Vectors are Stored in S5

The database uses S5's decentralized storage with the following structure:

1. **Content Addressing**: Each vector is stored with a CID (Content Identifier)
2. **CBOR Serialization**: All data is serialized using CBOR format for S5 compatibility
3. **Compression**: Optional zstd compression for large vectors
4. **Immutability**: Once stored, vectors cannot be modified (only new versions can be added)

### Storage Layout

```
/vectors/
  ├── recent/           # HNSW index data
  │   ├── nodes/        # Vector nodes
  │   └── graph/        # HNSW graph connections
  ├── historical/       # IVF index data
  │   ├── centroids/    # Cluster centroids
  │   └── inverted/     # Inverted lists
  └── metadata/         # Vector metadata
```

### S5 Client Configuration

```rust
pub struct S5Config {
    pub node_url: String,           // S5 portal URL
    pub api_key: Option<String>,    // Optional API key
    pub enable_compression: bool,   // Enable compression
    pub cache_size: usize,         // Local cache size
}
```

### Seed Phrase Management

The S5 seed phrase can be:
1. Set via `S5_SEED_PHRASE` environment variable
2. Loaded from a file via `S5_SEED_PHRASE_FILE` environment variable
3. Generated automatically and stored securely
4. Retrieved via admin API (with proper authentication)

#### Using Environment Variable
```bash
# Set custom seed phrase (must be 12 or 24 words)
export S5_SEED_PHRASE="your twelve word mnemonic seed phrase here for s5 storage access"
```

#### Using Seed Phrase File (Recommended)
```bash
# Create seed phrase file with proper permissions
echo "your twelve word seed phrase goes here like this example phrase" > ~/.s5-seed
chmod 600 ~/.s5-seed  # Restrict access to owner only

# Use file for configuration
export S5_SEED_PHRASE_FILE=~/.s5-seed
```

**Security Notes**:
- Seed phrases must contain exactly 12 or 24 words
- On Unix systems, seed phrase files with world-readable permissions will trigger a warning
- The file method takes precedence over the environment variable if both are set
- Seed phrases are never logged or exposed in API responses

### Portal Configuration

```bash
# Use default S5 portal
export S5_PORTAL_URL=https://s5.vup.cx

# Or use custom portal
export S5_PORTAL_URL=http://localhost:5524
```

## HAMT Sharding

### Automatic Activation

HAMT (Hash Array Mapped Trie) sharding automatically activates when:
- Vector count exceeds `HAMT_ACTIVATION_THRESHOLD` (default: 1000)
- Provides O(log n) lookup performance
- Maintains compatibility with S5's HAMT implementation

### Performance Characteristics

| Vector Count | Without HAMT | With HAMT   |
|-------------|--------------|-------------|
| < 1,000     | O(1)         | O(1)        |
| 10,000      | O(n)         | O(log n)    |
| 100,000     | O(n)         | O(log n)    |
| 1,000,000   | O(n)         | O(log n)    |

### Configuration Options

```bash
# Activation threshold
HAMT_ACTIVATION_THRESHOLD=1000

# Branching factor (affects tree depth)
HAMT_BRANCHING_FACTOR=32

# Maximum depth
HAMT_MAX_DEPTH=10
```

### HAMT Structure

```
Root Node
├─ Branch[0-31]
│  ├─ Branch[0-31]
│  │  └─ Leaf → Vector Data
│  └─ Leaf → Vector Data
└─ Branch[32-63]
   └─ Leaf → Vector Data
```

## Examples

### Basic Usage Examples

#### 1. Initialize Client (TypeScript)

```typescript
import { VectorDBClient } from 'fabstir-ai-vector-db';

const client = new VectorDBClient({
  apiUrl: 'http://localhost:7530',
  apiKey: process.env.VECTOR_DB_API_KEY
});

// Check health
const health = await client.health();
console.log('Database status:', health.status);
```

#### 2. Insert Vectors

```typescript
// Single vector insertion
const vector = await generateEmbedding("AI tutorial video");
const result = await client.insertVector({
  id: 'vec_001',
  vector: vector,
  metadata: {
    video_id: 'video_123',
    title: 'Introduction to AI',
    tags: ['ai', 'tutorial', 'beginner']
  }
});

// Batch insertion
const vectors = await Promise.all([
  generateEmbedding("Machine learning basics"),
  generateEmbedding("Deep learning fundamentals"),
  generateEmbedding("Neural networks explained")
]);

const batchResult = await client.batchInsert({
  vectors: vectors.map((vec, i) => ({
    id: `vec_${i + 100}`,
    vector: vec,
    metadata: {
      video_id: `video_${i + 100}`,
      title: `Lesson ${i + 1}`
    }
  }))
});
```

#### 3. Search Vectors

```typescript
// Basic search
const queryVector = await generateEmbedding("How do neural networks work?");
const results = await client.search({
  vector: queryVector,
  k: 5
});

// Advanced search with options
const advancedResults = await client.search({
  vector: queryVector,
  k: 10,
  filter: {
    tags: ['neural-networks', 'deep-learning']
  },
  options: {
    search_recent: true,
    search_historical: true,
    hnsw_ef: 100,        // Higher = better quality, slower
    ivf_n_probe: 32,     // More clusters = better quality, slower
    timeout_ms: 5000,
    include_metadata: true,
    score_threshold: 0.75
  }
});
```

### Advanced Queries

#### 1. Time-based Search

```typescript
// Search only recent vectors (last 24 hours)
const recentResults = await client.search({
  vector: queryVector,
  k: 10,
  options: {
    search_recent: true,
    search_historical: false,
    recent_threshold_override: 24 * 60 * 60 * 1000  // 24 hours in ms
  }
});
```

#### 2. Filtered Search

```typescript
// Complex metadata filtering
const filteredResults = await client.search({
  vector: queryVector,
  k: 20,
  filter: {
    $and: [
      { tags: { $in: ['ai', 'ml', 'deep-learning'] } },
      { duration_seconds: { $gte: 300, $lte: 1800 } },
      { model_name: 'text-embedding-ada-002' }
    ]
  }
});
```

#### 3. Streaming Updates

```typescript
// Subscribe to real-time updates
const eventSource = new EventSource('http://localhost:7530/stream/updates');

eventSource.addEventListener('vector_added', (event) => {
  const data = JSON.parse(event.data);
  console.log('New vector added:', data.id);
});

eventSource.addEventListener('migration_completed', (event) => {
  const data = JSON.parse(event.data);
  console.log(`Migration completed: ${data.vectors_migrated} vectors`);
});
```

### Integration with AI Applications

#### 1. OpenAI Integration

```typescript
import OpenAI from 'openai';
import { VectorDBClient } from 'fabstir-ai-vector-db';

const openai = new OpenAI();
const vectorDB = new VectorDBClient();

async function indexVideo(videoTranscript: string, metadata: any) {
  // Generate embedding
  const response = await openai.embeddings.create({
    model: "text-embedding-ada-002",
    input: videoTranscript
  });
  
  const embedding = response.data[0].embedding;
  
  // Store in vector database
  await vectorDB.insertVector({
    id: `vec_${metadata.video_id}`,
    vector: embedding,
    metadata: {
      ...metadata,
      model_name: 'text-embedding-ada-002',
      indexed_at: new Date().toISOString()
    }
  });
}

async function semanticSearch(query: string) {
  // Generate query embedding
  const response = await openai.embeddings.create({
    model: "text-embedding-ada-002",
    input: query
  });
  
  const queryEmbedding = response.data[0].embedding;
  
  // Search similar videos
  return await vectorDB.search({
    vector: queryEmbedding,
    k: 10,
    options: {
      include_metadata: true,
      score_threshold: 0.8
    }
  });
}
```

#### 2. LangChain Integration

```typescript
import { VectorDBStore } from 'fabstir-ai-vector-db/langchain';
import { OpenAIEmbeddings } from 'langchain/embeddings/openai';

// Create vector store
const vectorStore = new VectorDBStore({
  embeddings: new OpenAIEmbeddings(),
  apiUrl: 'http://localhost:7530'
});

// Add documents
await vectorStore.addDocuments([
  { pageContent: "AI fundamentals", metadata: { topic: "ai" } },
  { pageContent: "Machine learning basics", metadata: { topic: "ml" } }
]);

// Similarity search
const results = await vectorStore.similaritySearch("What is AI?", 5);
```

### Using with Different Embedding Models

#### 1. Cohere Embeddings

```typescript
import cohere from 'cohere-ai';

cohere.init(process.env.COHERE_API_KEY);

async function generateCohereEmbedding(text: string) {
  const response = await cohere.embed({
    texts: [text],
    model: 'embed-english-v3.0'
  });
  
  return response.body.embeddings[0];
}
```

#### 2. Sentence Transformers (Python)

```python
from sentence_transformers import SentenceTransformer
import requests

model = SentenceTransformer('all-MiniLM-L6-v2')

def index_video(video_text, metadata):
    # Generate embedding
    embedding = model.encode(video_text).tolist()
    
    # Store in vector database
    response = requests.post(
        'http://localhost:7530/vectors',
        json={
            'id': f"vec_{metadata['video_id']}",
            'vector': embedding,
            'metadata': metadata
        }
    )
    return response.json()
```

#### 3. Custom Embeddings

```typescript
// Using your own embedding model
async function customEmbedding(text: string): Promise<number[]> {
  // Your custom embedding logic
  const response = await fetch('http://your-model-api/embed', {
    method: 'POST',
    body: JSON.stringify({ text }),
    headers: { 'Content-Type': 'application/json' }
  });
  
  const data = await response.json();
  return data.embedding;
}

// Use with vector database
const embedding = await customEmbedding("Video content");
await vectorDB.insertVector({
  id: 'custom_vec_001',
  vector: embedding,
  metadata: {
    model_name: 'custom-model-v1',
    dimension: embedding.length
  }
});
```

## Performance & Scalability

### Benchmarks

Performance metrics on standard hardware (16 CPU cores, 32GB RAM):

| Operation | Vectors | Latency (p50) | Latency (p99) | Throughput |
|-----------|---------|---------------|---------------|------------|
| Insert    | 1M      | 0.5ms         | 2ms           | 2000 ops/s |
| Insert    | 10M     | 0.8ms         | 3ms           | 1250 ops/s |
| Search    | 1M      | 15ms          | 45ms          | 1000 QPS   |
| Search    | 10M     | 25ms          | 50ms          | 500 QPS    |

### Scaling Considerations

#### 1. Memory Usage

- **HNSW Index**: ~500 bytes per vector
- **IVF Index**: ~100 bytes per vector
- **Metadata**: Variable (typically 100-500 bytes)

Example memory requirements:
- 1M vectors: ~1GB RAM
- 10M vectors: ~10GB RAM
- 100M vectors: ~100GB RAM

#### 2. Storage Requirements

- **Raw Vectors**: 4 bytes × dimension per vector
- **Compressed**: ~60-70% of raw size with zstd
- **Indexes**: Additional 20-30% overhead

Example storage for 1536-dim vectors:
- 1M vectors: ~6GB (raw) → ~4GB (compressed)
- 10M vectors: ~60GB (raw) → ~40GB (compressed)

#### 3. Query Performance Optimization

```typescript
// Optimize for speed (lower quality)
const fastSearch = {
  hnsw_ef: 50,      // Lower ef = faster
  ivf_n_probe: 8,   // Fewer probes = faster
  timeout_ms: 1000
};

// Optimize for quality (slower)
const qualitySearch = {
  hnsw_ef: 200,     // Higher ef = better quality
  ivf_n_probe: 32,  // More probes = better quality
  timeout_ms: 5000
};

// Balanced approach
const balancedSearch = {
  hnsw_ef: 100,
  ivf_n_probe: 16,
  timeout_ms: 3000
};
```

### Best Practices

#### 1. Batch Operations

```typescript
// Good: Batch insert
await client.batchInsert({
  vectors: largeArrayOfVectors  // Insert 1000 at once
});

// Bad: Individual inserts
for (const vector of largeArrayOfVectors) {
  await client.insertVector(vector);  // Slow!
}
```

#### 2. Connection Pooling

```typescript
// Reuse client instances
const client = new VectorDBClient({
  apiUrl: 'http://localhost:7530',
  maxConnections: 10,
  keepAlive: true
});

// Use the same client instance across your application
export default client;
```

#### 3. Index Tuning

```bash
# For write-heavy workloads
HNSW_EF_CONSTRUCTION=100  # Lower for faster inserts
IVF_N_CLUSTERS=128        # Fewer clusters

# For read-heavy workloads
HNSW_EF_CONSTRUCTION=400  # Higher for better search quality
IVF_N_CLUSTERS=512        # More clusters for better distribution
```

#### 4. Migration Strategy

```typescript
// Configure automatic migration
const config = {
  recent_threshold: 7 * 24 * 60 * 60 * 1000,  // 7 days
  migration_batch_size: 1000,
  auto_migrate: true
};

// Or manually trigger migration during off-peak
await client.admin.migrate();
```

## Error Handling

### Common Errors and Solutions

#### 1. Configuration Errors

```typescript
// Error: S5_PORTAL_URL required for real mode
// Solution: Set the portal URL when using real mode
export S5_MODE=real
export S5_PORTAL_URL=https://s5.vup.cx

// Error: Invalid URL format for S5_PORTAL_URL: must start with http:// or https://
// Solution: Ensure URLs have proper protocol
export S5_PORTAL_URL=https://s5.vup.cx  // ✓ Correct
export S5_PORTAL_URL=s5.vup.cx          // ✗ Wrong

// Error: Invalid seed phrase: expected 12 or 24 words, got 10
// Solution: Use a valid BIP39 seed phrase with correct word count
export S5_SEED_PHRASE="twelve words go here for proper seed phrase format example test"
```

#### 2. Dimension Mismatch

```typescript
// Error: DimensionMismatch: expected 1536, got 384
// Solution: Ensure all vectors have the same dimension
const EXPECTED_DIMENSION = 1536;

function validateVector(vector: number[]): void {
  if (vector.length !== EXPECTED_DIMENSION) {
    throw new Error(`Vector must have ${EXPECTED_DIMENSION} dimensions`);
  }
}
```

#### 3. Duplicate Vector ID

```typescript
// Error: DuplicateVector: Vector with ID vec_123 already exists
// Solution: Use unique IDs or check existence first
try {
  await client.insertVector({ id, vector, metadata });
} catch (error) {
  if (error.code === 'DUPLICATE_VECTOR') {
    // Update existing vector instead
    await client.updateVector({ id, vector, metadata });
  }
}
```

#### 4. Index Not Initialized

```typescript
// Error: NotInitialized: Index not trained
// Solution: Initialize the index with training data
const trainingVectors = await loadTrainingData();
await client.admin.initializeIndex(trainingVectors);
```

#### 5. S5 Connection Issues

```typescript
// Error: NetworkError: Failed to connect to S5 portal
// Solution: Implement retry logic
async function retryOperation<T>(
  operation: () => Promise<T>,
  maxRetries = 3
): Promise<T> {
  for (let i = 0; i < maxRetries; i++) {
    try {
      return await operation();
    } catch (error) {
      if (i === maxRetries - 1) throw error;
      await new Promise(resolve => setTimeout(resolve, 1000 * Math.pow(2, i)));
    }
  }
  throw new Error('Max retries exceeded');
}
```

### Debugging Tips

#### 1. Enable Debug Logging

```bash
# Set environment variable
export RUST_LOG=vector_db=debug,tower_http=debug

# Or in .env file
RUST_LOG=vector_db=debug,tower_http=debug
```

#### 2. Monitor Performance

```typescript
// Use the statistics endpoint
const stats = await client.admin.getStatistics();
console.log('Vector distribution:', {
  recent: stats.recent_vectors,
  historical: stats.historical_vectors,
  memoryUsage: stats.memory_usage
});
```

#### 3. Health Checks

```typescript
// Regular health monitoring
setInterval(async () => {
  try {
    const health = await client.health();
    if (health.status !== 'healthy') {
      console.error('Database unhealthy:', health);
      // Trigger alerts
    }
  } catch (error) {
    console.error('Health check failed:', error);
  }
}, 30000);  // Every 30 seconds
```

#### 4. Trace Requests

```typescript
// Add request ID for tracing
const requestId = generateRequestId();
const result = await client.search({
  vector: queryVector,
  k: 10,
  headers: {
    'X-Request-ID': requestId
  }
});

// Check server logs for this request ID
```

## API Rate Limits and Quotas

### Default Limits

- **Request Rate**: 1000 requests/minute per IP
- **Batch Size**: Maximum 1000 vectors per batch
- **Request Size**: 10MB per request
- **Query Timeout**: 30 seconds
- **Connection Limit**: 100 concurrent connections

## Implementation Status

### Fully Implemented Endpoints
- ✅ `GET /health` - Health check
- ✅ `POST /vectors` - Insert single vector
- ✅ `POST /vectors/batch` - Batch insert vectors
- ✅ `GET /vectors/{id}` - Get vector by ID
- ✅ `DELETE /vectors/{id}` - Delete vector
- ✅ `POST /search` - Vector similarity search

### Partially Implemented Endpoints
These endpoints have placeholder implementations that return default/empty responses:
- ⚠️ `GET /admin/statistics` - Returns zeros (TODO: implement actual statistics)
- ⚠️ `POST /admin/migrate` - Returns zeros (TODO: implement migration logic)
- ⚠️ `POST /admin/rebalance` - Returns zeros (TODO: implement rebalancing)
- ⚠️ `POST /admin/backup` - Returns zeros (TODO: implement backup functionality)
- ⚠️ `GET /stream/updates` - Returns empty stream (TODO: implement SSE events)
- ⚠️ `GET /ws` - Returns status code only (TODO: implement WebSocket handler)

### Configuring Limits

```bash
# Environment variables
RATE_LIMIT_PER_MINUTE=1000
MAX_BATCH_SIZE=1000
MAX_REQUEST_SIZE=10485760
QUERY_TIMEOUT_MS=30000
MAX_CONNECTIONS=100
```

### Handling Rate Limits

```typescript
// Implement exponential backoff
async function handleRateLimit<T>(
  operation: () => Promise<T>
): Promise<T> {
  const maxRetries = 5;
  let delay = 1000; // Start with 1 second
  
  for (let i = 0; i < maxRetries; i++) {
    try {
      return await operation();
    } catch (error) {
      if (error.status === 429) { // Rate limited
        await new Promise(resolve => setTimeout(resolve, delay));
        delay *= 2; // Exponential backoff
        continue;
      }
      throw error;
    }
  }
  throw new Error('Rate limit exceeded after retries');
}
```

## Security Considerations

### Authentication

```typescript
// API Key authentication
const client = new VectorDBClient({
  apiUrl: 'http://localhost:7530',
  apiKey: process.env.VECTOR_DB_API_KEY
});

// Bearer token in headers
const response = await fetch('http://localhost:7530/vectors', {
  headers: {
    'Authorization': `Bearer ${apiKey}`,
    'Content-Type': 'application/json'
  }
});
```

### Encryption

- **Transport**: Use HTTPS in production
- **Storage**: Vectors are encrypted at rest in S5
- **Keys**: Store API keys and seed phrases securely

### Access Control

```typescript
// Example middleware for role-based access
app.use('/admin/*', requireRole('admin'));
app.use('/vectors/delete', requireRole('editor'));
app.use('/search', requireRole('reader'));
```

## Troubleshooting Guide

### Common Issues

1. **Slow Search Performance**
   - Check index statistics
   - Increase `hnsw_ef` for recent index
   - Increase `ivf_n_probe` for historical index
   - Consider adding more RAM

2. **High Memory Usage**
   - Enable compression
   - Reduce `HNSW_M` parameter
   - Implement pagination for large results
   - Monitor with `admin/statistics`

3. **Failed Migrations**
   - Check disk space
   - Verify S5 connectivity
   - Review migration logs
   - Manually trigger with smaller batches

4. **Connection Timeouts**
   - Increase timeout values
   - Check network connectivity
   - Verify S5 portal status
   - Implement retry logic

### Support Resources

- GitHub Issues: https://github.com/fabstir/fabstir-ai-vector-db/issues
- Documentation: https://docs.fabstir.ai/vector-db
- Community Discord: https://discord.gg/fabstir
- Email Support: support@fabstir.ai