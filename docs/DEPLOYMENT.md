# Vector DB Deployment Guide

## Quick Start After Reboot

```bash
# 1. Start Enhanced S5.js
cd ~/dev/Fabstir/partners/S5/GitHub/s5.js/
./start-real-s5.sh &

# 2. Start Vector DB Production
cd ~/dev/Fabstir/fabstir-vectordb/
docker-compose -f docker-compose.dev.yml up -d vector-db-prod

# 3. Verify
curl http://localhost:5522/health  # Enhanced S5.js
curl http://localhost:7533/api/v1/health  # Vector DB
```

## Architecture

```
Port 7533: Vector DB Production (lightweight container)
Port 5522: Enhanced S5.js (connected to s5.vup.cx)
Port 7530-7532: Vector DB Dev Container (for development)
```

## Key Configuration

- Timeout: 30 seconds (required for real S5 operations)
- Vector dimensions: 384 (for all-MiniLM-L6-v2)
- Storage: Real S5 network via Enhanced S5.js

## Troubleshooting

### Timeout Errors
- Check src/storage/enhanced_s5_storage.rs line 51
- Must be: `unwrap_or(30000)` not `unwrap_or(5000)`

### Port Conflicts
- Kill old processes: `fuser -k 7533/tcp`
- Check what's using ports: `lsof -i :7533`

### Container Issues
- Dev container mounts to: /workspace
- Production runs from: /app/vector-db-server