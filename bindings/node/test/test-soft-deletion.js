const { describe, test, before, after } = require('node:test');
const assert = require('node:assert');
const { VectorDbSession } = require('../index.js');
const { startS5Service } = require('./helpers/s5-service.cjs');

let s5Service = null;

before(async () => {
  console.log('Starting S5 service...');
  s5Service = await startS5Service({ port: 5528, mode: 'mock' });
  console.log('S5 service started on port 5528');
});

after(async () => {
  if (s5Service) {
    await s5Service.close();
  }
});

describe('Issue #3: Soft-Deleted Vectors Should Not Appear in Search', () => {
  let session;

  before(async () => {
    session = await VectorDbSession.create({
      s5Portal: 'http://127.0.0.1:5528',
      userSeedPhrase: 'soft-deletion-test-seed',
      sessionId: 'soft-deletion-session',
      encryptAtRest: false,
    });

    // Add test vectors
    const vectors = [];
    for (let i = 0; i < 20; i++) {
      vectors.push({
        id: `doc-${i}`,
        vector: Array(384).fill(0).map((_, idx) => Math.sin((idx + i) * 0.1)),
        metadata: {
          index: i,
          text: `Document ${i}`,
          category: i < 10 ? 'active' : 'archived',
          userId: 'user123'
        }
      });
    }
    await session.addVectors(vectors);
  });

  after(async () => {
    await session.destroy();
  });

  test('deleted vector should not appear in search results', async () => {
    const queryVector = Array(384).fill(0).map((_, idx) => Math.sin(idx * 0.1));

    // Search before deletion - should find doc-0
    const beforeResults = await session.search(queryVector, 20, { threshold: 0 });
    const doc0Before = beforeResults.find(r => r.id === 'doc-0');
    assert.ok(doc0Before, 'doc-0 should exist before deletion');

    // Delete doc-0
    await session.deleteVector('doc-0');

    // Search after deletion - should NOT find doc-0
    const afterResults = await session.search(queryVector, 20, { threshold: 0 });
    const doc0After = afterResults.find(r => r.id === 'doc-0');
    assert.strictEqual(doc0After, undefined, 'doc-0 should NOT appear after deletion');

    // Verify other vectors still exist
    assert.ok(afterResults.length >= 19, 'Should still have other vectors');
  });

  test('deleteByMetadata should remove matching vectors from search', async () => {
    const queryVector = Array(384).fill(0).map((_, idx) => Math.sin(idx * 0.1));

    // Search before deletion - should find archived documents (doc-10 to doc-19)
    const beforeResults = await session.search(queryVector, 20, { threshold: 0 });
    const archivedBefore = beforeResults.filter(r => {
      const index = parseInt(r.id.split('-')[1]);
      return index >= 10 && index < 20;
    });
    assert.ok(archivedBefore.length > 0, 'Should have archived documents before deletion');

    // Delete all archived documents
    const deleteResult = await session.deleteByMetadata({ category: 'archived' });
    assert.ok(deleteResult.deletedCount >= 9, `Should delete 9-10 archived documents, deleted ${deleteResult.deletedCount}`);

    // Search after deletion - should NOT find any archived documents
    const afterResults = await session.search(queryVector, 20, { threshold: 0 });
    const archivedAfter = afterResults.filter(r => {
      const index = parseInt(r.id.split('-')[1]);
      return index >= 10 && index < 20;
    });
    assert.strictEqual(archivedAfter.length, 0, 'Should NOT find archived documents after deletion');

    // Verify active documents still exist
    const activeDocs = afterResults.filter(r => {
      const index = parseInt(r.id.split('-')[1]);
      return index >= 1 && index < 10; // doc-0 was deleted in previous test
    });
    assert.ok(activeDocs.length >= 8, 'Active documents should still exist');
  });

  test('vacuum should physically remove deleted vectors', async () => {
    // Get stats before vacuum
    const statsBefore = await session.getStats();
    const deletedBefore = statsBefore.totalDeletedCount || 0;
    assert.ok(deletedBefore >= 10, `Should have ~11 deleted vectors before vacuum, got ${deletedBefore}`);

    // Vacuum to physically remove deleted vectors
    const vacuumStats = await session.vacuum();
    console.log('Vacuum stats:', vacuumStats);
    assert.ok(vacuumStats.totalRemoved >= 10, `Should remove ~11 vectors, removed ${vacuumStats.totalRemoved}`);

    // Get stats after vacuum
    const statsAfter = await session.getStats();
    const deletedAfter = statsAfter.totalDeletedCount || 0;
    assert.strictEqual(deletedAfter, 0, 'Should have 0 deleted vectors after vacuum');

    // Verify search still works and doesn't return deleted vectors
    const queryVector = Array(384).fill(0).map((_, idx) => Math.sin(idx * 0.1));
    const results = await session.search(queryVector, 20, { threshold: 0 });

    // Should only have active documents (doc-1 to doc-9)
    assert.ok(results.length >= 8 && results.length <= 9, `Should have 8-9 results, got ${results.length}`);

    // Verify no deleted documents appear
    for (const result of results) {
      const index = parseInt(result.id.split('-')[1]);
      assert.ok(index >= 1 && index < 10, `Result ${result.id} should be in active range`);
    }
  });
});
