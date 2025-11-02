const { describe, test, before, after } = require('node:test');
const assert = require('node:assert');
const { VectorDbSession } = require('../index.js');
const { startS5Service } = require('./helpers/s5-service.cjs');

let s5Service = null;

before(async () => {
  console.log('Starting S5 service...');
  s5Service = await startS5Service({ port: 5533, mode: 'mock' });
  console.log('S5 service started on port 5533');
});

after(async () => {
  if (s5Service) {
    await s5Service.close();
  }
});

describe('Small Dataset Tests - IVF Minimum Vector Requirement', () => {

  test('should work with 1-vector dataset', async () => {
    console.log('\n=== TEST 1: 1-Vector Dataset ===');

    const session = await VectorDbSession.create({
      s5Portal: 'http://127.0.0.1:5533',
      userSeedPhrase: 'test-seed-1vec',
      sessionId: 'test-1-vector',
      encryptAtRest: false
    });

    // Add single vector
    await session.addVectors([{
      id: 'doc-0',
      vector: new Array(384).fill(0).map((_, i) => Math.sin(i * 0.1)),
      metadata: { title: 'Single document' }
    }]);

    // Search should work with 1 vector
    const query = new Array(384).fill(0).map((_, i) => Math.sin(i * 0.1));
    const results = await session.search(query, 10, { threshold: 0 });

    console.log(`  1-vector search: ${results.length} results (expected 1)`);
    console.log(`  First result: ${results[0].id}, score: ${results[0].score.toFixed(4)}`);

    assert.strictEqual(results.length, 1, 'Should return 1 result');
    assert.strictEqual(results[0].id, 'doc-0', 'Should return the only vector');
    assert.ok(results[0].score > 0.99, 'Score should be near 1.0 for exact match');

    await session.destroy();
  });

  test('should work with 2-vector dataset', async () => {
    console.log('\n=== TEST 2: 2-Vector Dataset ===');

    const session = await VectorDbSession.create({
      s5Portal: 'http://127.0.0.1:5533',
      userSeedPhrase: 'test-seed-2vec',
      sessionId: 'test-2-vectors',
      encryptAtRest: false
    });

    // Add 2 vectors
    await session.addVectors([
      {
        id: 'doc-0',
        vector: new Array(384).fill(0).map((_, i) => Math.sin(i * 0.1)),
        metadata: { title: 'Document 0' }
      },
      {
        id: 'doc-1',
        vector: new Array(384).fill(0).map((_, i) => Math.cos(i * 0.1)),
        metadata: { title: 'Document 1' }
      }
    ]);

    // Search should work with 2 vectors
    const query = new Array(384).fill(0).map((_, i) => Math.sin(i * 0.1));
    const results = await session.search(query, 10, { threshold: 0 });

    console.log(`  2-vector search: ${results.length} results (expected 2)`);
    console.log(`  Results: ${results.map(r => `${r.id} (${r.score.toFixed(4)})`).join(', ')}`);

    assert.strictEqual(results.length, 2, 'Should return 2 results');
    assert.ok(results.some(r => r.id === 'doc-0'), 'Should include doc-0');
    assert.ok(results.some(r => r.id === 'doc-1'), 'Should include doc-1');

    await session.destroy();
  });

  test('should work with empty index (0 vectors)', async () => {
    console.log('\n=== TEST 3: Empty Index (0 Vectors) ===');

    const session = await VectorDbSession.create({
      s5Portal: 'http://127.0.0.1:5533',
      userSeedPhrase: 'test-seed-empty',
      sessionId: 'test-0-vectors',
      encryptAtRest: false
    });

    // Search on empty index
    const query = new Array(384).fill(0).map((_, i) => Math.sin(i * 0.1));
    const results = await session.search(query, 10, { threshold: 0 });

    console.log(`  Empty index search: ${results.length} results (expected 0)`);

    assert.strictEqual(results.length, 0, 'Should return 0 results for empty index');

    await session.destroy();
  });

  test('should handle gradual growth (1 → 10 → 100 vectors)', async () => {
    console.log('\n=== TEST 4: Gradual Dataset Growth ===');

    const session = await VectorDbSession.create({
      s5Portal: 'http://127.0.0.1:5533',
      userSeedPhrase: 'test-seed-growth',
      sessionId: 'test-gradual-growth',
      encryptAtRest: false
    });

    const query = new Array(384).fill(0).map((_, i) => Math.sin(i * 0.1));

    // Add 1 vector
    await session.addVectors([{
      id: 'doc-0',
      vector: new Array(384).fill(0).map((_, i) => Math.sin(i * 0.1)),
      metadata: { batch: 1 }
    }]);

    let results = await session.search(query, 10, { threshold: 0 });
    console.log(`  After 1 vector: ${results.length} results (expected 1)`);
    assert.strictEqual(results.length, 1, 'Should return 1 result');

    // Add 9 more vectors (total 10)
    const batch2 = [];
    for (let i = 1; i < 10; i++) {
      batch2.push({
        id: `doc-${i}`,
        vector: new Array(384).fill(0).map((_, idx) => Math.sin((idx + i) * 0.1)),
        metadata: { batch: 2 }
      });
    }
    await session.addVectors(batch2);

    results = await session.search(query, 10, { threshold: 0 });
    console.log(`  After 10 vectors: ${results.length} results (expected 10)`);
    assert.strictEqual(results.length, 10, 'Should return 10 results');

    // Add 90 more vectors (total 100)
    const batch3 = [];
    for (let i = 10; i < 100; i++) {
      batch3.push({
        id: `doc-${i}`,
        vector: new Array(384).fill(0).map((_, idx) => Math.sin((idx + i) * 0.1)),
        metadata: { batch: 3 }
      });
    }
    await session.addVectors(batch3);

    results = await session.search(query, 10, { threshold: 0 });
    console.log(`  After 100 vectors: ${results.length} results (expected 10, limited by k)`);
    assert.strictEqual(results.length, 10, 'Should return 10 results (limited by k)');

    await session.destroy();
  });

  test('should auto-train IVF when threshold reached', async () => {
    console.log('\n=== TEST 5: Auto-Train IVF at Threshold ===');

    const session = await VectorDbSession.create({
      s5Portal: 'http://127.0.0.1:5533',
      userSeedPhrase: 'test-seed-autotrain',
      sessionId: 'test-auto-train',
      encryptAtRest: false
    });

    // Add 5 vectors (below threshold, should use HNSW-only)
    const vectors5 = [];
    for (let i = 0; i < 5; i++) {
      vectors5.push({
        id: `doc-${i}`,
        vector: new Array(384).fill(0).map((_, idx) => Math.sin((idx + i) * 0.1)),
        metadata: { phase: 'hnsw-only' }
      });
    }
    await session.addVectors(vectors5);

    const query = new Array(384).fill(0).map((_, i) => Math.sin(i * 0.1));
    let results = await session.search(query, 5, { threshold: 0 });
    console.log(`  5 vectors (HNSW-only): ${results.length} results`);
    assert.strictEqual(results.length, 5, 'Should work with HNSW-only mode');

    // Add 10 more vectors (total 15, above threshold, should trigger IVF training)
    const vectors10 = [];
    for (let i = 5; i < 15; i++) {
      vectors10.push({
        id: `doc-${i}`,
        vector: new Array(384).fill(0).map((_, idx) => Math.sin((idx + i) * 0.1)),
        metadata: { phase: 'ivf-trained' }
      });
    }
    await session.addVectors(vectors10);

    results = await session.search(query, 10, { threshold: 0 });
    console.log(`  15 vectors (IVF trained): ${results.length} results`);
    assert.strictEqual(results.length, 10, 'Should work after IVF training');

    await session.destroy();
  });

  test('should handle delete operations on small datasets', async () => {
    console.log('\n=== TEST 6: Delete on Small Dataset ===');

    const session = await VectorDbSession.create({
      s5Portal: 'http://127.0.0.1:5533',
      userSeedPhrase: 'test-seed-delete',
      sessionId: 'test-delete-small',
      encryptAtRest: false
    });

    // Add 3 vectors
    await session.addVectors([
      {
        id: 'doc-0',
        vector: new Array(384).fill(0).map((_, i) => Math.sin(i * 0.1)),
        metadata: { title: 'Doc 0' }
      },
      {
        id: 'doc-1',
        vector: new Array(384).fill(0).map((_, i) => Math.cos(i * 0.1)),
        metadata: { title: 'Doc 1' }
      },
      {
        id: 'doc-2',
        vector: new Array(384).fill(0).map((_, i) => Math.sin(i * 0.2)),
        metadata: { title: 'Doc 2' }
      }
    ]);

    // Delete one vector
    await session.deleteVector('doc-1');

    // Search should return 2 vectors
    const query = new Array(384).fill(0).map((_, i) => Math.sin(i * 0.1));
    const results = await session.search(query, 10, { threshold: 0 });

    console.log(`  After delete: ${results.length} results (expected 2)`);
    console.log(`  Results: ${results.map(r => r.id).join(', ')}`);

    assert.strictEqual(results.length, 2, 'Should return 2 results after deletion');
    assert.ok(!results.some(r => r.id === 'doc-1'), 'Should not include deleted doc-1');

    await session.destroy();
  });

  test('should handle metadata updates on small datasets', async () => {
    console.log('\n=== TEST 7: Metadata Update on Small Dataset ===');

    const session = await VectorDbSession.create({
      s5Portal: 'http://127.0.0.1:5533',
      userSeedPhrase: 'test-seed-update',
      sessionId: 'test-update-small',
      encryptAtRest: false
    });

    // Add 2 vectors
    await session.addVectors([
      {
        id: 'doc-0',
        vector: new Array(384).fill(0).map((_, i) => Math.sin(i * 0.1)),
        metadata: { title: 'Original Title', status: 'draft' }
      },
      {
        id: 'doc-1',
        vector: new Array(384).fill(0).map((_, i) => Math.cos(i * 0.1)),
        metadata: { title: 'Another Doc', status: 'published' }
      }
    ]);

    // Update metadata
    await session.updateMetadata('doc-0', { title: 'Updated Title', status: 'published' });

    // Search and verify metadata
    const query = new Array(384).fill(0).map((_, i) => Math.sin(i * 0.1));
    const results = await session.search(query, 10, { threshold: 0 });

    console.log(`  After update: ${results.length} results`);
    const doc0 = results.find(r => r.id === 'doc-0');
    console.log(`  doc-0 metadata: ${JSON.stringify(doc0.metadata)}`);

    assert.strictEqual(doc0.metadata.title, 'Updated Title', 'Metadata should be updated');
    assert.strictEqual(doc0.metadata.status, 'published', 'Status should be updated');

    await session.destroy();
  });

  test('should persist small datasets correctly (save/load)', async () => {
    console.log('\n=== TEST 8: Small Dataset Persistence ===');

    const sessionId = 'test-persist-small';
    let cid;

    // Create session and add 2 vectors
    {
      const session = await VectorDbSession.create({
        s5Portal: 'http://127.0.0.1:5533',
        userSeedPhrase: 'test-seed-persist',
        sessionId,
        encryptAtRest: false
      });

      await session.addVectors([
        {
          id: 'doc-0',
          vector: new Array(384).fill(0).map((_, i) => Math.sin(i * 0.1)),
          metadata: { title: 'Persistent Doc 0' }
        },
        {
          id: 'doc-1',
          vector: new Array(384).fill(0).map((_, i) => Math.cos(i * 0.1)),
          metadata: { title: 'Persistent Doc 1' }
        }
      ]);

      // Save to S5
      cid = await session.saveToS5();
      console.log(`  Saved 2 vectors to S5 with CID: ${cid}`);

      await session.destroy();
    }

    // Load in new session
    {
      const session2 = await VectorDbSession.create({
        s5Portal: 'http://127.0.0.1:5533',
        userSeedPhrase: 'test-seed-persist',
        sessionId: `${sessionId}-reload`,
        encryptAtRest: false
      });

      await session2.loadUserVectors(cid);
      console.log(`  Loaded from CID: ${cid}`);

      // Search should return 2 vectors
      const query = new Array(384).fill(0).map((_, i) => Math.sin(i * 0.1));
      const results = await session2.search(query, 10, { threshold: 0 });

      console.log(`  After reload: ${results.length} results (expected 2)`);
      console.log(`  Results: ${results.map(r => `${r.id} (${r.metadata.title})`).join(', ')}`);

      assert.strictEqual(results.length, 2, 'Should return 2 results after reload');
      assert.ok(results.some(r => r.id === 'doc-0'), 'Should include doc-0');
      assert.ok(results.some(r => r.id === 'doc-1'), 'Should include doc-1');

      const doc0 = results.find(r => r.id === 'doc-0');
      assert.strictEqual(doc0.metadata.title, 'Persistent Doc 0', 'Metadata should persist');

      await session2.destroy();
    }
  });
});
