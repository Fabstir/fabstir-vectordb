const { describe, test, before, after } = require('node:test');
const assert = require('node:assert');
const { VectorDbSession } = require('../index.js');
const { startS5Service } = require('./helpers/s5-service.cjs');

let s5Service = null;

before(async () => {
  console.log('Starting S5 service...');
  s5Service = await startS5Service({ port: 5531, mode: 'mock' });
  console.log('S5 service started on port 5531');
});

after(async () => {
  if (s5Service) {
    await s5Service.close();
  }
});

describe('SDK Developer Reproduction Tests', () => {
  let session;

  before(async () => {
    session = await VectorDbSession.create({
      s5Portal: 'http://127.0.0.1:5531',
      userSeedPhrase: 'sdk-reproduction-seed',
      sessionId: 'sdk-reproduction-session',
      encryptAtRest: false,
    });
  });

  after(async () => {
    await session.destroy();
  });

  test('Issue #1: topK with 20 random vectors', async () => {
    console.log('\n=== Testing topK with 20 Random Vectors ===');

    // Add 20 vectors with RANDOM embeddings (exactly like SDK test)
    const vectors = [];
    for (let i = 0; i < 20; i++) {
      const values = new Array(384).fill(0).map(() => Math.random());
      vectors.push({
        id: `doc-${i}`,
        vector: values,
        metadata: { index: i }
      });
    }

    console.log(`[1] Adding ${vectors.length} vectors with random embeddings...`);
    await session.addVectors(vectors);
    console.log('  ✓ Added 20 vectors');

    // Query with first vector's values
    const queryVector = vectors[0].vector;

    // Test k=3
    console.log('[2] Testing k=3...');
    const results3 = await session.search(queryVector, 3, { threshold: 0 });
    console.log(`  Expected: 3, Got: ${results3.length}`);
    console.log(`  First few IDs: ${results3.slice(0, 3).map(r => r.id).join(', ')}`);
    console.log(`  First few scores: ${results3.slice(0, 3).map(r => r.score.toFixed(4)).join(', ')}`);

    assert.strictEqual(results3.length, 3, `Should return 3 results, got ${results3.length}`);

    // Test k=10
    console.log('[3] Testing k=10...');
    const results10 = await session.search(queryVector, 10, { threshold: 0 });
    console.log(`  Expected: 10, Got: ${results10.length}`);

    assert.strictEqual(results10.length, 10, `Should return 10 results, got ${results10.length}`);

    // Test k=100 (should cap at 20)
    console.log('[4] Testing k=100 (should cap at 20)...');
    const results100 = await session.search(queryVector, 100, { threshold: 0 });
    console.log(`  Expected: 20, Got: ${results100.length}`);

    assert.strictEqual(results100.length, 20, `Should return 20 results (max), got ${results100.length}`);

    console.log('  ✅ All topK assertions passed with random vectors!');
  });

  test('Issue #3: Soft-delete with 5 vectors', async () => {
    console.log('\n=== Testing Soft-Delete with 5 Vectors ===');

    // Add 5 vectors: 2 with status='delete', 3 with status='keep'
    const vectors = [];
    for (let i = 0; i < 5; i++) {
      const values = new Array(384).fill(0).map(() => Math.random());
      vectors.push({
        id: `soft-${i}`,
        vector: values,
        metadata: { index: i, status: i < 2 ? 'delete' : 'keep' }
      });
    }

    console.log('[1] Adding 5 vectors (2 to delete, 3 to keep)...');
    await session.addVectors(vectors);

    // BEFORE deletion: should return 5
    console.log('[2] Searching BEFORE deletion...');
    const beforeDelete = await session.search(vectors[0].vector, 10, { threshold: 0 });
    console.log(`  Before delete: ${beforeDelete.length} results (expected 5)`);
    assert.strictEqual(beforeDelete.length, 5, `Should have 5 results before delete, got ${beforeDelete.length}`);

    // Soft-delete vectors with status='delete'
    console.log('[3] Calling deleteByMetadata({ status: "delete" })...');
    const deleteResult = await session.deleteByMetadata({ status: 'delete' });
    console.log(`  Deleted count: ${deleteResult.deletedCount} (expected 2)`);
    console.log(`  Deleted IDs: ${deleteResult.deletedIds.join(', ')}`);

    assert.strictEqual(deleteResult.deletedCount, 2, `Should delete 2 vectors, got ${deleteResult.deletedCount}`);
    assert.ok(deleteResult.deletedIds.includes('soft-0'), 'Should include soft-0 in deleted IDs');
    assert.ok(deleteResult.deletedIds.includes('soft-1'), 'Should include soft-1 in deleted IDs');

    // AFTER deletion: should return 3 (only 'keep' vectors)
    console.log('[4] Searching AFTER deletion...');
    const afterDelete = await session.search(vectors[0].vector, 10, { threshold: 0 });
    console.log(`  After delete: ${afterDelete.length} results (expected 3)`);
    console.log(`  Result IDs: ${afterDelete.map(r => r.id).join(', ')}`);
    console.log(`  Statuses: ${afterDelete.map(r => r.metadata.status).join(', ')}`);

    assert.strictEqual(afterDelete.length, 3, `Should have 3 results after delete, got ${afterDelete.length}`);
    assert.ok(afterDelete.every(r => r.metadata.status === 'keep'), 'All results should have status=keep');

    console.log('  ✅ Soft-delete working correctly!');
  });
});
