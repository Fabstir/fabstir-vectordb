#!/usr/bin/env node

/**
 * S5 HTTP Service - Production-Ready Enhanced S5.js Wrapper
 *
 * Provides HTTP endpoints that wrap Enhanced S5.js filesystem API.
 * Supports both mock mode (for testing) and real mode (for production).
 *
 * This service is used by:
 * - VectorDB Rust native bindings (via EnhancedS5Storage)
 * - Node.js integration tests
 * - Production P2P host containers
 *
 * API Endpoints:
 * - PUT /s5/fs/:path - Store data (CBOR body)
 * - GET /s5/fs/:path - Retrieve data (returns bytes)
 * - DELETE /s5/fs/:path - Delete data
 * - GET /health - Health check
 *
 * Environment Variables:
 * - S5_MODE: 'mock' or 'real' (default: 'mock')
 * - S5_PORT: Service port (default: 5522)
 * - S5_PORTAL: S5 portal URL for real mode (default: none for mock)
 * - S5_SEED_PHRASE: User seed phrase (optional, generates if missing)
 */

import express from 'express';

const app = express();

// Configuration
const MODE = process.env.S5_MODE || 'mock';
const PORT = parseInt(process.env.S5_PORT || '5522', 10);
const PORTAL_URL = process.env.S5_PORTAL;
const SEED_PHRASE = process.env.S5_SEED_PHRASE;

// Middleware - parse raw body for CBOR data
app.use(express.raw({ type: '*/*', limit: '100mb' }));

// Storage backend (mode-dependent)
let s5Client = null;
let mockStorage = null;
let isInitialized = false;

/**
 * Initialize storage backend based on mode
 */
async function initializeStorage() {
  if (isInitialized) return;

  if (MODE === 'real') {
    // Real mode - use Enhanced S5.js
    try {
      // Dynamically import Enhanced S5.js (ESM)
      const { S5 } = await import('@s5-dev/s5js');

      console.log('Initializing Enhanced S5.js client...');

      // Create S5 instance with peer connection
      s5Client = await S5.create({
        initialPeers: PORTAL_URL ? [PORTAL_URL] : [
          'wss://z2DWuPbL5pweybXnEB618pMnV58ECj2VPDNfVGm3tFqBvjF@s5.ninja/s5/p2p'
        ]
      });

      // Generate or use provided seed phrase
      const seedPhrase = SEED_PHRASE || s5Client.generateSeedPhrase();

      // Recover identity
      await s5Client.recoverIdentityFromSeedPhrase(seedPhrase);

      // Register on portal if specified
      if (PORTAL_URL) {
        console.log(`Registering on portal: ${PORTAL_URL}`);
        await s5Client.registerOnNewPortal(PORTAL_URL);
      }

      // Initialize filesystem
      await s5Client.fs.ensureIdentityInitialized();

      console.log('Enhanced S5.js initialized successfully');
      isInitialized = true;
    } catch (error) {
      console.error('Failed to initialize Enhanced S5.js:', error);
      throw error;
    }
  } else {
    // Mock mode - use in-memory storage
    mockStorage = new Map();
    console.log('Initialized mock storage');
    isInitialized = true;
  }
}

/**
 * Health check endpoint
 */
app.get('/health', (req, res) => {
  res.json({
    status: 'ok',
    mode: MODE,
    initialized: isInitialized,
    port: PORT,
    portal: PORTAL_URL || 'none',
    storage_size: MODE === 'mock' ? mockStorage?.size : 'unknown'
  });
});

/**
 * Store data at path
 * PUT /s5/fs/:path
 * Body: Raw bytes (CBOR data)
 */
app.put('/s5/fs/:path(*)', async (req, res) => {
  try {
    await initializeStorage();

    const path = req.params.path;
    const data = req.body;

    console.log(`PUT /s5/fs/${path} - ${data.length} bytes`);

    if (MODE === 'real') {
      // Use Enhanced S5.js filesystem API
      // Data arrives as Buffer, convert to Uint8Array
      const uint8Data = new Uint8Array(data);
      await s5Client.fs.put(path, uint8Data);
    } else {
      // Mock mode - store in memory
      mockStorage.set(path, data);
    }

    res.json({ success: true });
  } catch (error) {
    console.error('PUT error:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * Retrieve data from path
 * GET /s5/fs/:path
 * Returns: Raw bytes
 */
app.get('/s5/fs/:path(*)', async (req, res) => {
  try {
    await initializeStorage();

    const path = req.params.path;
    console.log(`GET /s5/fs/${path}`);

    let data;

    if (MODE === 'real') {
      // Use Enhanced S5.js filesystem API
      const result = await s5Client.fs.get(path);

      if (result === undefined) {
        res.status(404).json({ error: 'Path not found' });
        return;
      }

      // Convert result to Buffer
      // s5.fs.get() returns Uint8Array for binary data
      if (result instanceof Uint8Array) {
        data = Buffer.from(result);
      } else if (typeof result === 'string') {
        data = Buffer.from(result, 'utf-8');
      } else {
        // For objects, return as-is (will be JSON stringified)
        res.json(result);
        return;
      }
    } else {
      // Mock mode - retrieve from memory
      data = mockStorage.get(path);

      if (!data) {
        res.status(404).json({ error: 'Path not found' });
        return;
      }
    }

    res.send(data);
  } catch (error) {
    console.error('GET error:', error);

    if (error.message && error.message.includes('not found')) {
      res.status(404).json({ error: 'Path not found' });
    } else {
      res.status(500).json({ error: error.message });
    }
  }
});

/**
 * Delete data at path
 * DELETE /s5/fs/:path
 */
app.delete('/s5/fs/:path(*)', async (req, res) => {
  try {
    await initializeStorage();

    const path = req.params.path;
    console.log(`DELETE /s5/fs/${path}`);

    if (MODE === 'real') {
      // Use Enhanced S5.js filesystem API
      const success = await s5Client.fs.delete(path);

      if (!success) {
        res.status(404).json({ error: 'Path not found' });
        return;
      }
    } else {
      // Mock mode - delete from memory
      const existed = mockStorage.has(path);
      mockStorage.delete(path);

      if (!existed) {
        res.status(404).json({ error: 'Path not found' });
        return;
      }
    }

    res.json({ success: true });
  } catch (error) {
    console.error('DELETE error:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * Start the HTTP server
 */
async function start() {
  try {
    // Initialize storage on startup
    await initializeStorage();

    app.listen(PORT, () => {
      console.log('='.repeat(60));
      console.log('S5 HTTP Service');
      console.log('='.repeat(60));
      console.log(`Mode:          ${MODE}`);
      console.log(`Port:          ${PORT}`);
      console.log(`Portal:        ${PORTAL_URL || 'none (mock mode)'}`);
      console.log(`Initialized:   ${isInitialized}`);
      console.log('='.repeat(60));
      console.log(`Listening on http://localhost:${PORT}`);
      console.log(`Health check: http://localhost:${PORT}/health`);
      console.log('='.repeat(60));
    });
  } catch (error) {
    console.error('Failed to start S5 HTTP service:', error);
    process.exit(1);
  }
}

/**
 * Graceful shutdown
 */
process.on('SIGINT', () => {
  console.log('\nShutting down S5 HTTP service...');
  process.exit(0);
});

process.on('SIGTERM', () => {
  console.log('\nShutting down S5 HTTP service...');
  process.exit(0);
});

// Export for programmatic use (tests)
export { app, initializeStorage };

// Start server if run directly
if (import.meta.url === `file://${process.argv[1]}`) {
  start();
}
