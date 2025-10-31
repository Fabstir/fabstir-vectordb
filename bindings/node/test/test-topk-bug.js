// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

const { describe, test, before, after } = require('node:test');
const assert = require('node:assert');
const { VectorDbSession } = require('../index.js');
const { startS5Service } = require('./helpers/s5-service.cjs');

// S5 service instance
let s5Service = null;

before(async () => {
  console.log('Starting S5 service...');
  s5Service = await startS5Service({ port: 5527, mode: 'mock' });
  console.log('S5 service started on port 5527');
});

after(async () => {
  if (s5Service) {
    await s5Service.close();
  }
});

describe('Bug: topK Parameter Not Respected', () => {
  test('should return k results when k < total vectors', async () => {
    console.log('\n=== Testing topK Parameter Bug ===\n');

    const config = {
      s5Portal: 'http://127.0.0.1:5527',
      userSeedPhrase: 'topk-bug-test-seed',
      sessionId: 'topk-bug-session',
      encryptAtRest: false,
    };

    const session = await VectorDbSession.create(config);

    // Add 20 vectors
    console.log('[1] Adding 20 vectors...');
    const vectors = [];
    for (let i = 0; i < 20; i++) {
      vectors.push({
        id: `doc-${i}`,
        vector: Array(384).fill(0).map((_, idx) => Math.sin((idx + i) * 0.1)),
        metadata: { index: i }
      });
    }

    await session.addVectors(vectors);
    console.log(`  ✓ Added 20 vectors`);

    // Check stats
    const stats = session.getStats();
    console.log(`  Stats: ${stats.vectorCount} total vectors`);
    console.log(`  HNSW: ${stats.hnswVectorCount}, IVF: ${stats.ivfVectorCount}`);

    // Test different k values
    const queryVector = vectors[0].vector;

    console.log('\n[2] Testing with k=3...');
    const results3 = await session.search(queryVector, 3, { threshold: 0 });
    console.log(`  Expected: 3, Got: ${results3.length}`);
    if (results3.length > 0) {
      console.log(`  First result: ${results3[0].id}`);
    }

    console.log('\n[3] Testing with k=10...');
    const results10 = await session.search(queryVector, 10, { threshold: 0 });
    console.log(`  Expected: 10, Got: ${results10.length}`);

    console.log('\n[4] Testing with k=100 (should return 20 since only 20 vectors)...');
    const results100 = await session.search(queryVector, 100, { threshold: 0 });
    console.log(`  Expected: 20, Got: ${results100.length}`);

    await session.destroy();

    // Assert expectations
    console.log('\n[5] Verifying results...');
    assert.strictEqual(results3.length, 3, `Expected 3 results with k=3, got ${results3.length}`);
    assert.strictEqual(results10.length, 10, `Expected 10 results with k=10, got ${results10.length}`);
    assert.strictEqual(results100.length, 20, `Expected 20 results with k=100, got ${results100.length}`);

    console.log('  ✅ All topK assertions passed!');
  });
});
