# Real S5 Integration Testing Guide

This guide explains how to set up and run tests against real Enhanced S5.js storage (not mock).

## Seed Phrase Requirements

Enhanced S5.js requires **proper BIP39 mnemonic seed phrases**:
- **12 words** (standard) or **24 words** (extended)
- Must be from the BIP39 word list
- Validation is enforced in **Real** mode only (skipped in Mock mode)

### Valid Seed Phrase Examples

```javascript
// 12-word seed phrase (recommended for testing)
const seedPhrase = 'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about';

// Another valid 12-word example
const seedPhrase = 'test test test test test test test test test test test junk';

// 24-word seed phrase (more secure, but overkill for testing)
const seedPhrase = 'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art';
```

### Invalid Seed Phrases (will fail validation in Real mode)

```javascript
// ❌ Too short
'test-vacuum-seed'

// ❌ Not BIP39 words
'integration-test-seed-phrase-0123456789abcdef'

// ❌ Arbitrary strings
'test-seed-phrase-for-unit-tests-12345678901234567890'
```

## Generating Seed Phrases

### Option 1: Use Enhanced S5.js Generator

```javascript
const { S5 } = require('s5js'); // Adjust import based on actual package

const s5 = await S5.create();
const seedPhrase = s5.generateSeedPhrase();
console.log('Generated seed phrase:', seedPhrase);
// Example output: "abandon ability able about above absent absorb abstract absurd abuse access accident"
```

### Option 2: Use Standard BIP39 Library

```bash
npm install bip39
```

```javascript
const bip39 = require('bip39');

// Generate new seed phrase
const mnemonic = bip39.generateMnemonic(); // 12 words
console.log('Seed phrase:', mnemonic);

// Validate seed phrase
const isValid = bip39.validateMnemonic(mnemonic);
console.log('Valid:', isValid); // true
```

### Option 3: Use Fixed Test Seed Phrases

For **reproducible testing**, use well-known BIP39 test phrases:

```javascript
// Standard test vector from BIP39 spec
const TEST_SEED = 'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about';

// Or use the "all test words" phrase
const TEST_SEED_ALT = 'test test test test test test test test test test test junk';
```

## Running Real S5 Tests

### Prerequisites

Enhanced S5.js is now available as **npm package @julesl23/s5js@0.9.0-beta**!

#### Option 1: Use Pre-built Test Server (Recommended)

1. **Install and Start Test Server**:
   ```bash
   cd test-s5-server
   npm install
   npm start
   ```

   This starts a local S5 server on port 5522 using the official npm package.

2. **Verify S5 Connectivity**:
   ```bash
   curl http://localhost:5522/health
   # Should return: {"status":"ok","version":"0.9.0-beta"}
   ```

#### Option 2: Use Enhanced S5.js Directly

If you have an existing Enhanced S5.js project:

1. **Start Your S5 Server**:
   ```bash
   # From your s5-node directory
   npm run dev  # Development mode on port 5522
   # or
   npm start    # Production mode
   ```

2. **Verify Connectivity**:
   ```bash
   curl http://localhost:5522/health
   # Should return: {"status":"ok"}
   ```

### Test Configuration

Update your test to use **real storage mode** and a **valid seed phrase**:

```javascript
const { VectorDbSession } = require('../index.js');

// Create session with real S5 storage
const session = await VectorDbSession.create({
  s5Portal: 'http://localhost:5522',

  // ✅ CORRECT: Valid 12-word BIP39 seed phrase
  userSeedPhrase: 'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about',

  sessionId: `real-s5-test-${Date.now()}`,

  // ✅ Use 'real' mode to test actual S5 persistence
  storageMode: 'real',
});
```

### Example: Vacuum Test with Real S5

```javascript
const { describe, it, before, after } = require('node:test');
const assert = require('node:assert');
const { VectorDbSession } = require('../index.js');

// Fixed seed phrase for reproducible testing
const REAL_S5_SEED = 'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about';

describe('Vacuum API - Real S5 Integration', () => {
  let session;

  before(async () => {
    console.log('# Creating session with real S5 storage...');
    session = await VectorDbSession.create({
      s5Portal: 'http://localhost:5522',
      userSeedPhrase: REAL_S5_SEED,
      sessionId: `vacuum-real-s5-${Date.now()}`,
      storageMode: 'real', // ← Real S5 mode
    });
    console.log('# ✓ Session created');
  });

  after(async () => {
    if (session) {
      await session.destroy();
      console.log('# ✓ Session destroyed');
    }
  });

  it('should vacuum and persist to real S5', async () => {
    // Add vectors
    const vectors = [];
    for (let i = 0; i < 50; i++) {
      vectors.push({
        id: `vec-${i}`,
        vector: Array(384).fill(0).map(() => Math.random()),
        metadata: { index: i },
      });
    }
    await session.addVectors(vectors);
    console.log('  ✓ Added 50 vectors');

    // Delete 10 vectors
    for (let i = 0; i < 10; i++) {
      await session.deleteVector(`vec-${i}`);
    }
    console.log('  ✓ Deleted 10 vectors');

    // Check deletion stats
    const statsBefore = await session.getStats();
    assert.strictEqual(statsBefore.totalDeletedCount, 10);
    console.log('  ✓ Stats show 10 deleted vectors');

    // Vacuum (cleanup in-memory)
    const vacuumStats = await session.vacuum();
    assert.strictEqual(vacuumStats.totalRemoved, 10);
    console.log(`  ✓ Vacuum removed ${vacuumStats.totalRemoved} vectors`);

    // Save to real S5 (SLOW: ~10-15 seconds for 100K vectors)
    console.log('  ⏳ Saving to real S5 (this may take 10-15 seconds)...');
    const startTime = Date.now();
    const cid = await session.saveToS5();
    const saveTime = Date.now() - startTime;
    console.log(`  ✓ Saved to S5 in ${saveTime}ms (CID: ${cid})`);

    // Verify stats after save
    const statsAfter = await session.getStats();
    assert.strictEqual(statsAfter.totalDeletedCount, 0);
    assert.strictEqual(statsAfter.vectorCount, 40);
    console.log('  ✓ Vacuumed state persisted to real S5');
  });

  it('should reload from real S5 without deleted vectors', async () => {
    // This test would require a separate session
    // and the CID from the previous test
    console.log('  ⚠ Test requires multi-session setup (skipped in this example)');
  });
});
```

## Performance Expectations

### Mock Mode (in-memory only)
- Vacuum: **< 100ms** for 1000 deletions
- Save/Load: **instant** (no network operations)
- Use for: Unit tests, fast iteration

### Real Mode (actual S5 persistence)
- Vacuum (cleanup): **< 100ms** for 1000 deletions
- Save to S5: **~10-15 seconds** for 100K vectors
  - Based on Enhanced S5.js benchmarks: ~800ms per file
  - 100K vectors = 10 chunks + manifest + schema = 12 files
  - Formula: 12 files × 800ms ≈ 10 seconds
- Load from S5: **~8-10 seconds** for 100K vectors
- Use for: Integration tests, E2E validation

### Bottleneck Analysis
Network latency dominates S5 operations (not bandwidth):
- Each file requires 8-10 registry operations
- Registry lookups add ~700-800ms per file
- Chunked storage minimizes file count (10K vectors/chunk)

## Running Tests

### Mock Mode (Fast - Default)
```bash
npm test test/vacuum.test.js
# Uses storageMode: 'mock', any seed phrase works
```

### Real S5 Mode (Slow - Integration)
```bash
# Terminal 1: Start S5 test server
cd test-s5-server
npm install  # First time only
npm start

# Terminal 2: Run real S5 tests
cd bindings/node
npm test test/vacuum-real-s5.test.js
# Uses storageMode: 'real', requires valid BIP39 seed phrase
```

**Quick Start** (One command):
```bash
# From project root
cd test-s5-server && npm install && npm start &
sleep 2 && cd ../bindings/node && npm test test/vacuum-real-s5.test.js
```

## Troubleshooting

### Error: "Invalid seed phrase: expected 12 or 24 words"
**Cause**: Using arbitrary string instead of BIP39 mnemonic in Real mode

**Fix**: Use proper 12-word seed phrase:
```javascript
userSeedPhrase: 'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about'
```

### Error: "S5 connection failed"
**Cause**: S5 server not running or wrong port

**Fix**:
```bash
# Check S5 server is running
curl http://localhost:5522/health

# From container, use host.docker.internal
s5Portal: 'http://host.docker.internal:5522'
```

### Error: "Timeout after 30000ms"
**Cause**: S5 operation taking too long (expected for large datasets)

**Fix**:
- Reduce dataset size for testing
- Increase timeout in config (if supported)
- Use mock mode for unit tests

### Tests fail with "seed phrase too short"
**Cause**: Mock mode tests incorrectly using real S5

**Fix**: Verify `storageMode: 'mock'` in test config

## Best Practices

1. **Use Mock Mode for Unit Tests**:
   - Fast iteration (no network)
   - Any seed phrase works
   - Focus on logic, not persistence

2. **Use Real Mode for Integration Tests**:
   - Validate actual S5 persistence
   - Test encryption, chunking, CID generation
   - Use fixed seed phrases for reproducibility

3. **Separate Test Files**:
   - `*.test.js` - Mock mode (fast, runs in CI)
   - `*.real-s5.test.js` - Real mode (slow, manual testing)

4. **Use Fixed Seed Phrases**:
   - Reproducible test data
   - Consistent CID generation
   - Easier debugging

5. **Document Performance**:
   - Note expected times for real S5 operations
   - Help developers understand why tests are slow
   - Set realistic expectations

## Example Test Files

### Unit Test (Mock Mode)
```javascript
// vacuum.test.js - Fast, runs in CI
const session = await VectorDbSession.create({
  s5Portal: 'http://localhost:5522',
  userSeedPhrase: 'any-string-works-in-mock-mode',
  sessionId: `test-${Date.now()}`,
  storageMode: 'mock', // ← Fast, no real S5 needed
});
```

### Integration Test (Real Mode)
```javascript
// vacuum-real-s5.test.js - Slow, manual testing
const REAL_S5_SEED = 'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about';

const session = await VectorDbSession.create({
  s5Portal: 'http://localhost:5522',
  userSeedPhrase: REAL_S5_SEED, // ← Valid BIP39 required
  sessionId: `integration-${Date.now()}`,
  storageMode: 'real', // ← Real S5, needs server running
});
```

## Quick Start Guide

### 1. Install S5 Test Server
```bash
cd test-s5-server
npm install
```

### 2. Start S5 Server (Terminal 1)
```bash
npm start
# Wait for: "Ready for vector database integration tests!"
```

### 3. Run Real S5 Tests (Terminal 2)
```bash
cd bindings/node
npm test test/vacuum-real-s5.test.js
```

### Expected Output
```
# Creating session with REAL S5 storage...
# ✓ Session created with real S5 storage
✓ Added 50 vectors
✓ Initial stats: 0 deleted, 50 active
✓ Deleted 10 vectors
✓ Stats show 10 deleted, 40 active
✓ Vacuum completed in 45ms
✓ Vacuum removed 10 vectors
  - HNSW: 10
  - IVF: 0
✓ After vacuum: 0 deleted, 40 active
⏱  Saving to REAL S5 (this may take 10-15 seconds)...
✓ Saved to S5 in 2847ms (2.8s)
✓ CID: bafkreig...
```

## References

- [Enhanced S5.js npm Package](https://www.npmjs.com/package/@julesl23/s5js) - **@julesl23/s5js@0.9.0-beta**
- [BIP39 Specification](https://github.com/bitcoin/bips/blob/master/bip-0039.mediawiki)
- [BIP39 Word List](https://github.com/bitcoin/bips/blob/master/bip-0039/english.txt)
- [Enhanced S5.js Benchmarks](https://github.com/julesl23/s5.js/blob/main/docs/BENCHMARKS.md)
- [S5.js API Documentation](/workspace/docs/s5js-reference/API.md)
- [S5 Test Server README](/workspace/test-s5-server/README.md)
