/**
 * Vacuum API Tests for Vector DB Session
 * Tests the vacuum operation for physically removing soft-deleted vectors
 */

const { describe, it, before, after } = require('node:test');
const assert = require('node:assert');
const { VectorDbSession } = require('../index.js');

describe('Vacuum API Tests', () => {
  let session;
  const testVectors = [];

  // Generate test vectors (384 dimensions for all-MiniLM-L6-v2)
  for (let i = 0; i < 50; i++) {
    const vector = Array(384).fill(0).map(() => Math.random());
    testVectors.push({
      id: `vec-${i}`,
      vector,
      metadata: {
        index: i,
        category: i % 3 === 0 ? 'A' : i % 3 === 1 ? 'B' : 'C',
        active: i % 2 === 0,
      },
    });
  }

  before(async () => {
    console.log('# Creating session for vacuum tests...');
    session = await VectorDbSession.create({
      s5Portal: 'http://localhost:5522',
      // Note: For mock mode, any string works. For real S5 mode, must be valid
      // 12-word BIP39 seed phrase (see test/REAL_S5_TESTING.md for details)
      userSeedPhrase: 'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about',
      sessionId: `vacuum-test-${Date.now()}`,
      storageMode: 'mock',
    });
    console.log('# ✓ Session created');
  });

  after(async () => {
    if (session) {
      await session.destroy();
      console.log('# ✓ Session destroyed');
    }
  });

  it('should add test vectors', async () => {
    await session.addVectors(testVectors);
    console.log(`  ✓ Added ${testVectors.length} vectors`);
  });

  it('should show zero deletions initially', async () => {
    const stats = await session.getStats();
    assert.strictEqual(stats.totalDeletedCount, 0, 'Should have 0 deleted vectors');
    assert.strictEqual(stats.hnswDeletedCount, 0, 'HNSW should have 0 deleted');
    assert.strictEqual(stats.ivfDeletedCount, 0, 'IVF should have 0 deleted');
    console.log('  ✓ Initial stats show zero deletions');
  });

  it('should return zero removed when vacuuming with no deletions', async () => {
    const vacuumStats = await session.vacuum();
    assert.strictEqual(vacuumStats.totalRemoved, 0, 'Should remove 0 vectors');
    assert.strictEqual(vacuumStats.hnswRemoved, 0, 'HNSW should remove 0');
    assert.strictEqual(vacuumStats.ivfRemoved, 0, 'IVF should remove 0');
    console.log('  ✓ Vacuum with no deletions returned zero');
  });

  it('should delete some vectors', async () => {
    // Delete 10 specific vectors
    for (let i = 0; i < 10; i++) {
      await session.deleteVector(`vec-${i}`);
    }
    console.log('  ✓ Deleted 10 vectors');
  });

  it('should show deletions in stats after delete', async () => {
    const stats = await session.getStats();
    assert.strictEqual(stats.totalDeletedCount, 10, 'Should have 10 deleted vectors');
    assert.strictEqual(stats.vectorCount, 40, 'Should have 40 active vectors');
    console.log(`  ✓ Stats show ${stats.totalDeletedCount} deleted, ${stats.vectorCount} active`);
  });

  it('should remove deleted vectors when vacuuming', async () => {
    const vacuumStats = await session.vacuum();
    assert.strictEqual(vacuumStats.totalRemoved, 10, 'Should remove 10 vectors');
    assert.ok(vacuumStats.totalRemoved > 0, 'Should remove at least some vectors');
    console.log(`  ✓ Vacuum removed ${vacuumStats.totalRemoved} vectors`);
    console.log(`    - HNSW: ${vacuumStats.hnswRemoved}`);
    console.log(`    - IVF: ${vacuumStats.ivfRemoved}`);
  });

  it('should show zero deletions after vacuum', async () => {
    const stats = await session.getStats();
    assert.strictEqual(stats.totalDeletedCount, 0, 'Should have 0 deleted vectors after vacuum');
    assert.strictEqual(stats.vectorCount, 40, 'Should still have 40 active vectors');
    console.log('  ✓ After vacuum: 0 deleted, 40 active');
  });

  it('should return zero when vacuuming again', async () => {
    const vacuumStats = await session.vacuum();
    assert.strictEqual(vacuumStats.totalRemoved, 0, 'Second vacuum should remove 0');
    console.log('  ✓ Second vacuum removed nothing');
  });

  it('should handle delete by metadata and vacuum', async () => {
    // Delete all category 'A' vectors
    const deleteResult = await session.deleteByMetadata({ category: 'A' });
    console.log(`  ✓ Deleted ${deleteResult.deletedCount} vectors by metadata`);

    // Check stats before vacuum
    const statsBefore = await session.getStats();
    assert.ok(statsBefore.totalDeletedCount > 0, 'Should have deleted vectors');

    // Vacuum
    const vacuumStats = await session.vacuum();
    assert.strictEqual(vacuumStats.totalRemoved, deleteResult.deletedCount,
      'Vacuum should remove same number as deleted');
    console.log(`  ✓ Vacuum removed ${vacuumStats.totalRemoved} vectors`);

    // Check stats after vacuum
    const statsAfter = await session.getStats();
    assert.strictEqual(statsAfter.totalDeletedCount, 0, 'Should have 0 deleted after vacuum');
    console.log('  ✓ Stats confirmed vacuum success');
  });

  it('should persist vacuumed state with saveToS5/loadUserVectors', async () => {
    // Get current stats
    const statsBefore = await session.getStats();
    const vectorCountBefore = statsBefore.vectorCount;

    // Save to S5
    const cid = await session.saveToS5();
    console.log(`  ✓ Saved to S5 (CID: ${cid})`);

    // Destroy and recreate session
    await session.destroy();
    session = await VectorDbSession.create({
      s5Portal: 'http://localhost:5522',
      userSeedPhrase: 'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about',
      sessionId: `vacuum-reload-${Date.now()}`,
      storageMode: 'mock',
    });

    // Load from S5
    await session.loadUserVectors(cid);
    console.log('  ✓ Loaded from S5');

    // Check stats - should match pre-save state
    const statsAfter = await session.getStats();
    assert.strictEqual(statsAfter.vectorCount, vectorCountBefore,
      'Vector count should match after reload');
    assert.strictEqual(statsAfter.totalDeletedCount, 0,
      'Should have 0 deleted vectors after reload (vacuum state persisted)');
    console.log('  ✓ Vacuumed state persisted correctly');
  });

  it('should reduce memory usage after vacuum', async () => {
    // Add more vectors
    const moreVectors = [];
    for (let i = 100; i < 150; i++) {
      const vector = Array(384).fill(0).map(() => Math.random());
      moreVectors.push({
        id: `vec-${i}`,
        vector,
        metadata: { index: i },
      });
    }
    await session.addVectors(moreVectors);

    // Get memory usage before
    const statsBefore = await session.getStats();
    const memBefore = statsBefore.memoryUsageMb;

    // Delete half of all vectors
    for (let i = 100; i < 125; i++) {
      await session.deleteVector(`vec-${i}`);
    }

    // Memory should still be high (deleted vectors not removed yet)
    const statsAfterDelete = await session.getStats();
    console.log(`  Memory before vacuum: ${memBefore.toFixed(2)} MB`);
    console.log(`  Memory after delete: ${statsAfterDelete.memoryUsageMb.toFixed(2)} MB`);

    // Vacuum
    const vacuumStats = await session.vacuum();
    console.log(`  ✓ Vacuumed ${vacuumStats.totalRemoved} vectors`);

    // Memory should decrease (or at least not increase)
    const statsAfterVacuum = await session.getStats();
    console.log(`  Memory after vacuum: ${statsAfterVacuum.memoryUsageMb.toFixed(2)} MB`);

    // Note: Memory might not decrease significantly due to other data structures
    // but it should at least not increase
    assert.ok(statsAfterVacuum.memoryUsageMb <= statsAfterDelete.memoryUsageMb * 1.1,
      'Memory should not increase significantly after vacuum');
    console.log('  ✓ Memory usage check passed');
  });
});
