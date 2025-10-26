# @fabstir/vector-db-native

Native Node.js bindings for Fabstir Vector Database.

## Installation

```bash
npm install /path/to/fabstir-vector-db-native-0.1.0.tgz
```

## Quick Start

```javascript
const { VectorDbSession } = require('@fabstir/vector-db-native');

async function main() {
  const session = await VectorDbSession.create({
    s5Portal: 'http://localhost:5522',  // S5 HTTP service endpoint
    userSeedPhrase: 'your-blockchain-derived-seed-phrase',
    sessionId: 'my-chat-session',
  });

  try {
    // Add vectors with native object metadata (no JSON.stringify needed!)
    await session.addVectors([
      {
        id: 'doc1',
        vector: [0.1, 0.2, ...],  // Your embedding
        metadata: {
          text: 'Hello world',
          timestamp: Date.now(),
          userId: 'user123'
        }
      },
      {
        id: 'doc2',
        vector: [0.3, 0.4, ...],
        metadata: { text: 'Another document', tags: ['important'] }
      },
      {
        id: 'doc3',
        vector: [0.5, 0.6, ...],
        metadata: { text: 'Third document', views: 100 }
      }
    ]);

    // Search for similar vectors
    const queryVector = [0.15, 0.25, ...];  // Your query embedding
    const results = await session.search(queryVector, 5, {
      threshold: 0.7,           // Minimum similarity score
      includeVectors: false     // Don't return full vectors (saves bandwidth)
    });

    // Results include metadata as native objects
    results.forEach(result => {
      console.log(`ID: ${result.id}`);
      console.log(`Score: ${result.score}`);
      console.log(`Text: ${result.metadata.text}`);  // Direct access!
    });

    // Save to S5 decentralized storage
    const cid = await session.saveToS5();
    console.log(`Saved to S5 with CID: ${cid}`);

  } finally {
    // CRITICAL: Always destroy session to free memory
    await session.destroy();
  }
}
```

## API

See `index.d.ts` for full TypeScript definitions.

### VectorDbSession

Main class for managing vector sessions.

#### Static Methods

- `create(config: VectorDBConfig): Promise<VectorDbSession>` - Create new session with S5 storage

#### Instance Methods

- `addVectors(vectors: VectorInput[]): Promise<void>` - Add vectors to index with native object metadata
- `search(queryVector: number[], k: number, options?: SearchOptions): Promise<SearchResult[]>` - Search for similar vectors
- `loadUserVectors(cid: string, options?: LoadOptions): Promise<void>` - ✅ Load vectors from S5 storage
- `saveToS5(): Promise<string>` - ✅ Save index to S5 (returns CID/path identifier)
- `getStats(): SessionStats` - Get real-time session statistics (vector count, memory usage)
- `destroy(): Promise<void>` - **CRITICAL**: Clear memory and free resources

## Usage Examples

### Basic In-Memory Session

```javascript
const { VectorDbSession } = require('@fabstir/vector-db-native');

const session = await VectorDbSession.create({
  s5Portal: 'http://localhost:5522',
  userSeedPhrase: 'your-seed-phrase',
  sessionId: 'basic-session',
});

// Add vectors
await session.addVectors([
  { id: 'v1', vector: [0.1, 0.2, 0.3], metadata: { title: 'Doc 1' } },
  { id: 'v2', vector: [0.4, 0.5, 0.6], metadata: { title: 'Doc 2' } },
  { id: 'v3', vector: [0.7, 0.8, 0.9], metadata: { title: 'Doc 3' } }
]);

// Search
const results = await session.search([0.2, 0.3, 0.4], 2);
console.log(results);  // Returns top 2 similar vectors

await session.destroy();
```

### S5 Persistence Workflow

```javascript
// Session 1: Create and save
const session1 = await VectorDbSession.create({
  s5Portal: 'http://localhost:5522',
  userSeedPhrase: 'user-seed-phrase',
  sessionId: 'persistent-session',
});

await session1.addVectors([...myVectors]);
const cid = await session1.saveToS5();
console.log(`Saved with ID: ${cid}`);  // e.g., "persistent-session"
await session1.destroy();

// Session 2: Load from S5 (even on different host!)
const session2 = await VectorDbSession.create({
  s5Portal: 'http://localhost:5522',
  userSeedPhrase: 'user-seed-phrase',  // Same user
  sessionId: 'new-session',
});

await session2.loadUserVectors(cid);  // Load from saved CID
const stats = session2.getStats();
console.log(`Loaded ${stats.vectorCount} vectors`);

const results = await session2.search(queryVector, 5);
await session2.destroy();
```

### Working with Metadata

```javascript
// Metadata is stored as native JavaScript objects
await session.addVectors([
  {
    id: 'doc1',
    vector: embedding1,
    metadata: {
      title: 'Product Review',
      rating: 4.5,
      tags: ['electronics', 'review'],
      author: { name: 'Alice', verified: true },
      timestamp: Date.now()
    }
  }
]);

// Search results return metadata as objects
const results = await session.search(queryVector, 10);
results.forEach(result => {
  console.log(result.metadata.title);           // Direct property access
  console.log(result.metadata.author.name);     // Nested objects work
  console.log(result.metadata.tags.join(', ')); // Arrays work
});
```

## Requirements

- Node.js >= 16
- Linux x64 or ARM64

## Development

### Build

```bash
# Install dependencies
npm install

# Development build
npm run build:debug

# Production build
npm run build

# Run tests
npm test
```

### Create Tarball

```bash
npm run build
npm pack
```

## Integration Guide

For detailed integration instructions, see `../../docs/sdk-reference/VECTOR_DB_INTEGRATION.md`.

## Implementation Status

**Phase 1-5: Complete** ✅ Production Ready

All features implemented and tested:
- ✅ Session management (create, destroy)
- ✅ Add vectors with auto-initialization
- ✅ Search with similarity scoring and thresholds
- ✅ Native object metadata (no JSON.stringify needed!)
- ✅ Real-time statistics (vector count, memory usage, index distribution)
- ✅ **S5 persistence** - Save/load to decentralized storage
- ✅ Hybrid indexing (HNSW for recent + IVF for historical data)
- ✅ Memory tracking and cleanup

**Test Coverage:** 28/28 tests passing (10 test suites)

## Features

**✅ What Works:**
- Create session with HybridIndex (HNSW + IVF)
- Add vectors (any dimension, auto-validated)
- Search for similar vectors with configurable threshold
- Native JavaScript object metadata (direct property access)
- Get real-time statistics (vector counts, memory usage, index distribution)
- **Save index to S5 decentralized storage** (`saveToS5()`)
- **Load index from S5** (`loadUserVectors()`)
- Round-trip persistence (save → load preserves all data)
- Multi-session support (multiple sessions can load same data)
- Proper memory cleanup on destroy()

**Performance:**
- Binary size: 8.2 MB (optimized with LTO)
- Memory efficient: ~3.7 KB for 3 vectors
- Fast search: < 50ms typical latency

## License

MIT
