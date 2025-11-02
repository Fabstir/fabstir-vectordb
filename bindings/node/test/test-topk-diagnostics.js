const { describe, test, before, after } = require('node:test');
const assert = require('node:assert');
const { VectorDbSession } = require('../index.js');
const { startS5Service } = require('./helpers/s5-service.cjs');

let s5Service = null;

before(async () => {
  console.log('Starting S5 service...');
  s5Service = await startS5Service({ port: 5532, mode: 'mock' });
  console.log('S5 service started on port 5532');
});

after(async () => {
  if (s5Service) {
    await s5Service.close();
  }
});

describe('topK Diagnostic Tests', () => {

  test('Diagnostic 1: Explicit threshold=0', async () => {
    console.log('\n=== TEST 1: Explicit threshold=0 ===');

    const session = await VectorDbSession.create({
      s5Portal: 'http://127.0.0.1:5532',
      userSeedPhrase: 'test-seed',
      sessionId: 'diag1-threshold-0',
      encryptAtRest: false
    });

    // Add 20 vectors with random embeddings
    const vectors = [];
    for (let i = 0; i < 20; i++) {
      vectors.push({
        id: `doc-${i}`,
        vector: new Array(384).fill(0).map(() => Math.random()),
        metadata: { index: i }
      });
    }

    await session.addVectors(vectors);

    // Search with query from dataset, EXPLICIT threshold=0
    const query = vectors[0].vector;
    const results = await session.search(query, 10, { threshold: 0 });

    console.log(`With threshold=0, results: ${results.length} (expected 10)`);
    console.log(`First score: ${results[0].score.toFixed(4)}`);
    if (results.length > 1) {
      console.log(`Last score: ${results[results.length - 1].score.toFixed(4)}`);
    }

    if (results.length === 1) {
      console.log('  ❌ BUG CONFIRMED: topK ignores threshold=0, returns only 1 result');
    } else if (results.length === 10) {
      console.log('  ✅ CORRECT: threshold=0 returns 10 results');
    }

    await session.destroy();
    assert.strictEqual(results.length, 10, `Should return 10 results with threshold=0, got ${results.length}`);
  });

  test('Diagnostic 2: Query vector NOT in dataset', async () => {
    console.log('\n=== TEST 2: Query Not in Dataset ===');

    const session = await VectorDbSession.create({
      s5Portal: 'http://127.0.0.1:5532',
      userSeedPhrase: 'test-seed',
      sessionId: 'diag2-new-query',
      encryptAtRest: false
    });

    // Add 20 vectors
    const vectors = [];
    for (let i = 0; i < 20; i++) {
      vectors.push({
        id: `doc-${i}`,
        vector: new Array(384).fill(0).map(() => Math.random()),
        metadata: { index: i }
      });
    }

    await session.addVectors(vectors);

    // Search with NEW query (not in dataset)
    const newQuery = new Array(384).fill(0).map(() => Math.random());
    const results = await session.search(newQuery, 10, { threshold: 0 });

    console.log(`With new query (not in dataset), results: ${results.length} (expected 10)`);
    if (results.length > 0) {
      console.log(`First score: ${results[0].score.toFixed(4)}`);
      console.log(`Last score: ${results[results.length - 1].score.toFixed(4)}`);
    }

    if (results.length === 1) {
      console.log('  ❌ BUG: Returns only 1 result with low-similarity query');
    } else if (results.length === 10) {
      console.log('  ✅ CORRECT: Returns 10 results even with low similarities');
    }

    await session.destroy();
    assert.strictEqual(results.length, 10, `Should return 10 results, got ${results.length}`);
  });

  test('Diagnostic 3: Without any options (check default threshold)', async () => {
    console.log('\n=== TEST 3: No Options (Default Behavior) ===');

    const session = await VectorDbSession.create({
      s5Portal: 'http://127.0.0.1:5532',
      userSeedPhrase: 'test-seed',
      sessionId: 'diag3-defaults',
      encryptAtRest: false
    });

    // Add 20 vectors
    const vectors = [];
    for (let i = 0; i < 20; i++) {
      vectors.push({
        id: `doc-${i}`,
        vector: new Array(384).fill(0).map(() => Math.random()),
        metadata: { index: i }
      });
    }

    await session.addVectors(vectors);

    // Search WITHOUT any options (test defaults)
    const query = vectors[0].vector;
    const resultsNoOptions = await session.search(query, 10);

    console.log(`Without options, results: ${resultsNoOptions.length}`);
    if (resultsNoOptions.length > 0) {
      console.log(`First score: ${resultsNoOptions[0].score.toFixed(4)}`);
      if (resultsNoOptions.length > 1) {
        console.log(`Last score: ${resultsNoOptions[resultsNoOptions.length - 1].score.toFixed(4)}`);
      }
    }

    // Search WITH threshold=0
    const resultsWithThreshold = await session.search(query, 10, { threshold: 0 });
    console.log(`With threshold=0, results: ${resultsWithThreshold.length}`);

    if (resultsNoOptions.length < resultsWithThreshold.length) {
      const impliedThreshold = resultsWithThreshold[resultsNoOptions.length]?.score || 0;
      console.log(`  ⚠️ Default threshold detected! Implied value: ~${impliedThreshold.toFixed(2)}`);
    } else {
      console.log('  ✅ No default threshold (or threshold=0 by default)');
    }

    await session.destroy();
    // We expect at least SOME results
    assert.ok(resultsNoOptions.length > 0, 'Should return at least some results');
  });

  test('Diagnostic 4: SDK reproduction scenario', async () => {
    console.log('\n=== TEST 4: Exact SDK Scenario ===');

    const session = await VectorDbSession.create({
      s5Portal: 'http://127.0.0.1:5532',
      userSeedPhrase: 'test-seed',
      sessionId: 'diag4-sdk-exact',
      encryptAtRest: false
    });

    // Replicate EXACT SDK test scenario
    const vectors = Array.from({ length: 20 }, (_, i) => ({
      id: `doc-${i}`,
      vector: new Array(384).fill(0).map(() => Math.random()),
      metadata: { index: i }
    }));

    console.log('[1] Adding 20 vectors with random embeddings...');
    await session.addVectors(vectors);

    // Query with first vector's values (exact match should exist)
    const query = vectors[0].vector;

    console.log('[2] Searching with k=3...');
    const results3 = await session.search(query, 3);
    console.log(`  Expected: 3, Got: ${results3.length}`);
    if (results3.length > 0) {
      console.log(`  Scores: ${results3.map(r => r.score.toFixed(4)).join(', ')}`);
    }

    console.log('[3] Searching with k=10...');
    const results10 = await session.search(query, 10);
    console.log(`  Expected: 10, Got: ${results10.length}`);

    console.log('[4] Searching with k=10, threshold=0...');
    const results10Thresh0 = await session.search(query, 10, { threshold: 0 });
    console.log(`  Expected: 10, Got: ${results10Thresh0.length}`);

    if (results10.length < results10Thresh0.length) {
      console.log('  ⚠️ Default threshold IS filtering results!');
      console.log(`  Without threshold: ${results10.length}, With threshold=0: ${results10Thresh0.length}`);
    }

    await session.destroy();
    assert.strictEqual(results3.length, 3, `k=3 should return 3, got ${results3.length}`);
    assert.strictEqual(results10.length, 10, `k=10 should return 10, got ${results10.length}`);
  });
});
