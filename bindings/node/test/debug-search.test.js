const { describe, test, before, after } = require('node:test');
const assert = require('node:assert');
const { VectorDbSession } = require('../index.js');
const { startS5Service } = require('./helpers/s5-service.cjs');

// S5 service instance
let s5Service = null;

before(async () => {
  console.log('Starting S5 service...');
  s5Service = await startS5Service({ port: 5526, mode: 'mock' });
  console.log('S5 service started on port 5526');
});

after(async () => {
  if (s5Service) {
    await s5Service.close();
  }
});

describe('Debug: Search After Load', () => {
  test('minimal test - 100 vectors', async () => {
    console.log('\n=== Minimal Debug Test: 100 Vectors ===\n');

    const config = {
      s5Portal: 'http://127.0.0.1:5526',
      userSeedPhrase: 'debug-test-seed',
      sessionId: 'debug-session',
      encryptAtRest: true,
      chunkSize: 50, // 50 vectors per chunk = 2 chunks
    };

    // Phase 1: Create session and add 100 vectors
    console.log('[1] Creating session and adding 100 vectors...');
    const session1 = await VectorDbSession.create(config);

    const vectors = [];
    for (let i = 0; i < 100; i++) {
      vectors.push({
        id: `vec-${i}`,
        vector: Array(384).fill(0).map((_, idx) => Math.sin((idx + i) * 0.1)),
        metadata: { index: i, text: `Document ${i}` }
      });
    }

    await session1.addVectors(vectors);
    console.log('  ✓ Added 100 vectors');

    // Check stats before save
    const statsBefore = session1.getStats();
    console.log(`  Stats before save: ${statsBefore.vectorCount} vectors`);

    // Phase 2: Save to S5
    console.log('\n[2] Saving to S5...');
    const cid = await session1.saveToS5();
    console.log(`  ✓ Saved with CID: ${cid}`);

    // Destroy session 1
    await session1.destroy();
    console.log('  ✓ Session 1 destroyed');

    // Phase 3: Load from S5
    console.log('\n[3] Loading from S5 in new session...');
    const session2 = await VectorDbSession.create({
      ...config,
      sessionId: 'debug-session-load',
    });

    await session2.loadUserVectors(cid, { lazyLoad: true });
    console.log('  ✓ Index loaded');

    // Check stats after load
    const statsAfter = session2.getStats();
    console.log(`  Stats after load: ${statsAfter.vectorCount} vectors`);
    console.log(`  HNSW vectors: ${statsAfter.hnswVectorCount || 0}`);
    console.log(`  IVF vectors: ${statsAfter.ivfVectorCount || 0}`);

    // Phase 4: Search
    console.log('\n[4] Testing search...');

    // Search for vector 0 (should be nearly perfect match)
    const queryVector = vectors[0].vector;
    console.log(`  Query: Looking for vec-0 using its own vector`);

    const results = await session2.search(queryVector, 5);
    console.log(`  ✓ Search returned ${results.length} results`);

    if (results.length > 0) {
      console.log(`  Top result: id=${results[0].id}, score=${results[0].score.toFixed(6)}`);
      console.log(`  Metadata: ${JSON.stringify(results[0].metadata)}`);
    } else {
      console.log(`  ⚠️  NO RESULTS RETURNED!`);
      console.log(`  This indicates the search is not working after load.`);
    }

    // Try searching without threshold
    console.log('\n[5] Searching with threshold=0...');
    const resultsNoThreshold = await session2.search(queryVector, 5, { threshold: 0 });
    console.log(`  ✓ Search returned ${resultsNoThreshold.length} results`);

    await session2.destroy();

    // Verify we got results
    assert.ok(results.length > 0, 'Should return at least 1 result after load');
    assert.strictEqual(results[0].id, 'vec-0', 'First result should be vec-0');
  });
});
