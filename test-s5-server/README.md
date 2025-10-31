# Enhanced S5.js Test Server

Local HTTP server providing S5 storage functionality for vector database integration testing.

## Features

- Uses **@julesl23/s5js@0.9.0-beta** npm package
- HTTP API on port 5522 (configurable)
- Supports multiple S5 identities (per seed phrase)
- RESTful endpoints for PUT/GET/DELETE operations
- Health check endpoint
- CORS enabled for local testing

## Installation

```bash
cd test-s5-server
npm install
```

## Usage

### Start Server

```bash
npm start
# or
npm run dev
```

Server will start on `http://localhost:5522`

### Verify Server is Running

```bash
curl http://localhost:5522/health
# Expected: {"status":"ok","version":"0.9.0-beta"}
```

## API Endpoints

### Health Check

```bash
GET /health
GET /s5/health

Response: {"status":"ok","version":"0.9.0-beta"}
```

### Store Data

```bash
PUT /s5/{path}
Content-Type: multipart/form-data

Form fields:
- seedPhrase: 12 or 24-word BIP39 seed phrase
- path: Storage path (e.g., "home/vectors/chunk-0.cbor")
- data: Data to store (binary or text)

Response: {"success":true,"path":"...","size":1234}
```

### Retrieve Data

```bash
GET /s5/{path}
Header: X-Seed-Phrase: <seed phrase>

Response: Binary data or 404 if not found
```

### Delete Data

```bash
DELETE /s5/{path}
Header: X-Seed-Phrase: <seed phrase>

Response: {"success":true}
```

## Environment Variables

```bash
PORT=5522        # Server port (default: 5522)
HOST=0.0.0.0     # Server host (default: 0.0.0.0)
NODE_ENV=development  # Enable stack traces in errors
```

## Example Usage with Vector DB

```javascript
const { VectorDbSession } = require('@fabstir/vector-db-native');

// Start this server first (npm start)

// Create session pointing to local S5 server
const session = await VectorDbSession.create({
  s5Portal: 'http://localhost:5522',
  userSeedPhrase: 'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about',
  sessionId: 'test-session',
  storageMode: 'real', // Use real S5 persistence
});

// Add vectors
await session.addVectors([...]);

// Save to S5 (will hit this server)
const cid = await session.saveToS5();
console.log('Saved to S5:', cid);

// Load from S5
await session.loadUserVectors(cid);
```

## Integration with Vector DB Tests

The vector database tests can now run real S5 integration tests:

```bash
# Terminal 1: Start S5 server
cd test-s5-server
npm start

# Terminal 2: Run real S5 tests
cd bindings/node
npm test test/vacuum-real-s5.test.js
```

## Architecture

```
┌─────────────────┐
│ Vector DB Tests │
│  (Node.js)      │
└────────┬────────┘
         │ HTTP (port 5522)
         ▼
┌─────────────────┐
│  S5 Test Server │
│  (This package) │
└────────┬────────┘
         │ S5.js API
         ▼
┌─────────────────┐
│ @julesl23/s5js  │
│  (npm package)  │
└────────┬────────┘
         │ WebSocket
         ▼
┌─────────────────┐
│  S5 P2P Network │
│   (s5.ninja)    │
└─────────────────┘
```

## Troubleshooting

### Error: "Cannot find module '@julesl23/s5js'"

**Fix**: Run `npm install` in test-s5-server directory

### Error: "Address already in use"

**Fix**: Another process is using port 5522
```bash
# Find and kill process on port 5522
lsof -ti:5522 | xargs kill -9

# Or change port
PORT=5523 npm start
```

### Error: "Connection refused"

**Fix**: Ensure server is running before tests
```bash
curl http://localhost:5522/health
```

### Slow operations (>30 seconds)

**Cause**: Network latency to S5 P2P network is expected

**Expected performance**:
- PUT: ~800ms per file (registry operations)
- GET: ~700ms per file
- 100K vectors (12 files): ~10-15 seconds

## Development

### Debug Mode

```bash
NODE_ENV=development npm start
```

Enables detailed error stack traces.

### Monitor Requests

Server logs all requests:
```
PUT /s5/home/vectors/chunk-0.cbor
✓ Stored data at: home/vectors/chunk-0.cbor
GET /s5/home/vectors/chunk-0.cbor
✓ Retrieved data from: home/vectors/chunk-0.cbor
```

## References

- [Enhanced S5.js Package](https://www.npmjs.com/package/@julesl23/s5js)
- [Enhanced S5.js Benchmarks](https://github.com/julesl23/s5.js/blob/main/docs/BENCHMARKS.md)
- [Vector DB Real S5 Testing Guide](../bindings/node/test/REAL_S5_TESTING.md)
