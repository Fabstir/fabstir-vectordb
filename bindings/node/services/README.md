# S5 HTTP Service

Production-ready HTTP service that wraps Enhanced S5.js for VectorDB persistence.

## Overview

This service provides HTTP endpoints that the VectorDB Rust native bindings use to store and retrieve vector indices on the S5 decentralized network.

```
┌─────────────────────────────────────┐
│  Node.js App                        │
│  (SDK or Host Container)            │
└──────────────┬──────────────────────┘
               │ require('@fabstir/vector-db-native')
               ↓
┌─────────────────────────────────────┐
│  VectorDB Native Bindings (Rust)    │
│  via napi-rs                        │
└──────────────┬──────────────────────┘
               │ HTTP localhost:5522
               ↓
┌─────────────────────────────────────┐
│  S5 HTTP Service                    │  ← THIS SERVICE
│  (Node.js Express)                  │
└──────────────┬──────────────────────┘
               │ @s5-dev/s5js
               ↓
┌─────────────────────────────────────┐
│  S5 Network                         │
│  (s5.vup.cx, s5.ninja, etc.)       │
└─────────────────────────────────────┘
```

## Why This Service Exists

**The VectorDB core is written in Rust**, which cannot directly call JavaScript libraries. The Rust code makes HTTP calls to this service, which wraps the Enhanced S5.js library and handles actual S5 network communication.

This architecture enables:
- ✅ Reusable across different deployment scenarios
- ✅ Same service for testing and production
- ✅ Decentralized storage via S5 network
- ✅ User data isolation (via seed phrases)

## Modes

### Mock Mode (Testing)
- **Purpose**: Fast tests without network calls
- **Storage**: In-memory (Map)
- **Startup**: ~100ms
- **Use Case**: Unit tests, integration tests

### Real Mode (Production)
- **Purpose**: Production P2P host containers
- **Storage**: S5 network via Enhanced S5.js
- **Startup**: ~2-5 seconds (network connection)
- **Use Case**: Production deployments

## Installation

```bash
cd bindings/node/services
npm install
```

## Usage

### Start in Mock Mode (Testing)

```bash
npm run start:mock
```

Or:

```bash
S5_MODE=mock node s5-http-service.js
```

### Start in Real Mode (Production)

```bash
S5_MODE=real \
S5_PORTAL=https://s5.vup.cx \
S5_SEED_PHRASE="your-seed-phrase-here" \
node s5-http-service.js
```

Or:

```bash
npm run start:real
```

### Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `S5_MODE` | No | `mock` | Mode: `mock` or `real` |
| `S5_PORT` | No | `5522` | HTTP server port |
| `S5_PORTAL` | Real mode | none | S5 portal URL (e.g., `https://s5.vup.cx`) |
| `S5_SEED_PHRASE` | No | Generated | User seed phrase (generates if missing) |

## API Endpoints

### PUT /s5/fs/:path
Store data at path.

**Request:**
- Method: `PUT`
- URL: `http://localhost:5522/s5/fs/{path}`
- Body: Raw bytes (CBOR data)
- Headers: `Content-Type: application/cbor`

**Response:**
```json
{ "success": true }
```

### GET /s5/fs/:path
Retrieve data from path.

**Request:**
- Method: `GET`
- URL: `http://localhost:5522/s5/fs/{path}`

**Response:**
- Body: Raw bytes
- Status: `404` if not found

### DELETE /s5/fs/:path
Delete data at path.

**Request:**
- Method: `DELETE`
- URL: `http://localhost:5522/s5/fs/{path}`

**Response:**
```json
{ "success": true }
```

**Status:**
- `404` if path doesn't exist
- `200` on success

### GET /health
Health check endpoint.

**Response:**
```json
{
  "status": "ok",
  "mode": "mock",
  "initialized": true,
  "port": 5522,
  "portal": "none",
  "storage_size": 42
}
```

## For SDK Developers

### Local Development

1. **Start S5 service** (in one terminal):
   ```bash
   cd bindings/node/services
   npm run start:mock
   ```

2. **Run your app** (in another terminal):
   ```javascript
   const { VectorDbSession } = require('@fabstir/vector-db-native');

   const session = await VectorDbSession.create({
     s5Portal: 'http://localhost:5522',
     userSeedPhrase: 'your-test-seed-phrase',
     sessionId: 'test-session'
   });

   // Add vectors, search, save/load...
   ```

### Production Deployment (Docker)

**Dockerfile:**
```dockerfile
FROM node:20

WORKDIR /app

# Install dependencies
COPY package.json .
RUN npm install

# Copy service files
COPY services/ ./services/
COPY src/ ./src/

# Expose service port
EXPOSE 5522

# Start both processes (use PM2 or similar)
CMD ["npm", "start"]
```

**docker-compose.yml:**
```yaml
version: '3.8'
services:
  vector-host:
    build: .
    ports:
      - "3000:3000"  # Your app
      - "5522:5522"  # S5 service
    environment:
      - S5_MODE=real
      - S5_PORTAL=https://s5.vup.cx
      - NODE_ENV=production
```

### Process Manager (PM2)

**ecosystem.config.js:**
```javascript
module.exports = {
  apps: [
    {
      name: 's5-service',
      script: './services/s5-http-service.js',
      env: {
        S5_MODE: 'real',
        S5_PORT: '5522',
        S5_PORTAL: 'https://s5.vup.cx'
      }
    },
    {
      name: 'api-server',
      script: './src/server.js',
      env: {
        PORT: '3000'
      }
    }
  ]
};
```

Start:
```bash
pm2 start ecosystem.config.js
pm2 save
```

## Testing

The service is automatically started by tests:

```javascript
const { startS5Service } = require('./helpers/s5-service.cjs');

let s5Service;

before(async () => {
  s5Service = await startS5Service({ port: 5522, mode: 'mock' });
});

after(async () => {
  await s5Service.close();
});
```

Run tests:
```bash
cd bindings/node
npm test
```

## Architecture Notes

### Why HTTP?

- **Rust ↔ JavaScript boundary**: Rust can't directly call JS
- **Language agnostic**: Any language can use the service
- **Simple deployment**: Standard HTTP server
- **Easy debugging**: Use curl, Postman, etc.

### Performance

- **Mock mode**: In-memory, < 1ms latency
- **Real mode**: Network-dependent, ~50-200ms typical
- **Caching**: Service caches frequently accessed data

### Security

For production:
- **Use HTTPS**: Encrypt traffic
- **Firewall rules**: Only allow localhost access
- **Seed phrase**: Store securely (env vars, secrets manager)
- **Per-user isolation**: Each user has own seed phrase

## Troubleshooting

### Service won't start

1. Check port availability:
   ```bash
   lsof -i :5522
   ```

2. Kill existing process:
   ```bash
   fuser -k 5522/tcp
   ```

3. Check logs for errors

### Connection refused

1. Verify service is running:
   ```bash
   curl http://localhost:5522/health
   ```

2. Check firewall rules
3. Verify port in config matches

### Tests failing

1. Ensure service starts:
   ```bash
   npm run start:mock
   ```

2. Run tests with verbose output:
   ```bash
   node --test --test-reporter=verbose
   ```

3. Check S5 service logs

## Future Enhancements

- [ ] Per-request authentication (seed phrase in headers)
- [ ] Rate limiting
- [ ] Compression (gzip)
- [ ] Metrics/monitoring
- [ ] WebSocket support for streaming
- [ ] Multi-user session management

## License

MIT
