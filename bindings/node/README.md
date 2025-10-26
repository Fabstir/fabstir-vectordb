# @fabstir/vector-db-native

Native Node.js bindings for Fabstir Vector Database.

## Installation

```bash
npm install /path/to/fabstir-vector-db-native-0.1.0.tgz
```

## Quick Start

```javascript
const { VectorDBSession } = require('@fabstir/vector-db-native');

async function main() {
  const session = await VectorDBSession.create({
    s5Portal: 'http://localhost:5524',
    userSeedPhrase: 'your-seed-phrase',
    sessionId: 'session-123',
  });

  try {
    // Add vectors
    await session.addVectors([
      {
        id: 'doc1',
        vector: [...], // 384-dim embedding
        metadata: { text: 'Hello world' }
      }
    ]);

    // Search
    const results = await session.search(queryVector, 5);
    console.log(results);
  } finally {
    await session.destroy();
  }
}
```

## API

See `index.d.ts` for full TypeScript definitions.

### VectorDBSession

Main class for managing vector sessions.

#### Static Methods

- `create(config: VectorDBConfig): Promise<VectorDBSession>` - Create new session

#### Instance Methods

- `loadUserVectors(cid: string, options?: LoadOptions): Promise<void>` - Load vectors from S5
- `search(queryVector: number[], k: number, options?: SearchOptions): Promise<SearchResult[]>` - Search for similar vectors
- `addVectors(vectors: VectorInput[]): Promise<void>` - Add vectors to index
- `saveToS5(): Promise<string>` - Save index to S5 (returns CID)
- `getStats(): SessionStats` - Get session statistics
- `destroy(): Promise<void>` - **CRITICAL**: Clear memory

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

**Phase 1: Infrastructure** ✅ Complete
- napi-rs setup
- TypeScript definitions
- Build system

**Phase 2: Core Implementation** ✅ Complete
- ✅ Session management (create, destroy)
- ✅ Add vectors with auto-initialization
- ✅ Search with similarity scoring
- ✅ Metadata storage and retrieval
- ✅ Statistics (vector count, memory usage, index distribution)
- ⏸️ S5 load/save (requires serialization - Phase 3)

## Current Capabilities

**What Works:**
- Create session with HybridIndex
- Add vectors (any dimension, validated)
- Search for similar vectors
- Get real-time statistics
- Store and retrieve metadata

**Limitations:**
- In-memory only (no persistence yet)
- `loadUserVectors()` and `saveToS5()` throw "not implemented" errors
- Metadata must be JSON strings (use `JSON.stringify()` when adding, `JSON.parse()` when retrieving)

## Next Phase

**Phase 3: S5 Persistence** will add:
- Serialize/deserialize HybridIndex
- Real S5 storage integration
- Load/save vector indices to decentralized storage

## License

MIT
