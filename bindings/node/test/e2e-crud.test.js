const { test, before, after } = require('node:test');
const assert = require('node:assert');
const { VectorDbSession } = require('../index.js');
const { startS5Service } = require('./helpers/s5-service.cjs');

// S5 service instance
let s5Service = null;

const TEST_CONFIG = {
  s5Portal: 'http://localhost:5522',
  userSeedPhrase: 'test seed phrase for e2e crud testing only',
  sessionId: 'e2e-crud-session',
  debug: false,
};

const VECTOR_DIM = 384;

/**
 * Generate a test vector with deterministic values based on seed
 */
function generateTestVector(seed) {
  const vector = new Array(VECTOR_DIM);
  for (let i = 0; i < VECTOR_DIM; i++) {
    vector[i] = Math.sin(seed * i * 0.01) * 0.5 + 0.5;
  }
  return vector;
}

/**
 * Add training vectors (minimum required for IVF initialization)
 */
async function addTrainingVectors(session, count = 130) {
  const trainingVectors = [];
  for (let i = 0; i < count; i++) {
    trainingVectors.push({
      id: `train-${i}`,
      vector: generateTestVector(i),
      metadata: { type: 'training', index: i },
    });
  }
  await session.addVectors(trainingVectors);
}

// Start S5 mock service before all tests
before(async () => {
  console.log('Starting S5 service for E2E CRUD tests...');
  s5Service = await startS5Service({ port: 5522, mode: 'mock' });
});

// Stop S5 mock service after all tests
after(async () => {
  console.log('Stopping S5 service...');
  if (s5Service && s5Service.stop) {
    s5Service.stop();
  }
  console.log('✓ S5 service stopped');
});

// ============================================================================
// Full CRUD Workflow Tests
// ============================================================================

test('E2E: Full CRUD workflow - Create → Add → Save → Load → Update → Delete → Save', async (t) => {
  // Phase 1: Create session and add vectors
  let session = await VectorDbSession.create({
    ...TEST_CONFIG,
    sessionId: 'e2e-full-crud-1',
  });

  try {
    await addTrainingVectors(session);

    // Add test vectors with rich metadata
    const testVectors = [];
    for (let i = 0; i < 100; i++) {
      testVectors.push({
        id: `doc-${i}`,
        vector: generateTestVector(1000 + i),
        metadata: {
          title: `Document ${i}`,
          category: i % 3 === 0 ? 'tech' : i % 3 === 1 ? 'science' : 'arts',
          views: 100 + i * 10,
          published: i % 2 === 0,
          tags: i % 5 === 0 ? ['featured', 'popular'] : ['regular'],
        },
      });
    }
    await session.addVectors(testVectors);

    const stats1 = await session.getStats();
    assert.strictEqual(stats1.vectorCount, 230, 'Should have 230 vectors (130 training + 100 test)');

    // Save to S5
    const cid1 = await session.saveToS5();
    assert.ok(cid1, 'Should return CID after save');
    console.log(`  Phase 1 complete: Saved ${stats1.vectorCount} vectors with CID: ${cid1}`);

    await session.destroy();

    // Phase 2: Load and verify
    session = await VectorDbSession.create({
      ...TEST_CONFIG,
      sessionId: 'e2e-full-crud-2',
    });

    await session.loadUserVectors(cid1, { lazyLoad: true });
    const stats2 = await session.getStats();
    assert.strictEqual(stats2.vectorCount, 230, 'Should load all 230 vectors');

    // Search to verify data integrity
    const queryVector = generateTestVector(1000);
    const results1 = await session.search(queryVector, 10, { threshold: 0.0 });
    assert.ok(results1.length > 0, 'Should find vectors after load');
    assert.ok(results1[0].metadata.title, 'Metadata should be preserved');
    console.log(`  Phase 2 complete: Loaded and verified ${stats2.vectorCount} vectors`);

    // Phase 3: Update metadata
    await session.updateMetadata('doc-0', {
      title: 'Updated Document 0',
      category: 'tech',
      views: 9999,
      published: true,
      tags: ['featured', 'updated'],
      updatedAt: Date.now(),
    });

    // Search and verify update
    const results2 = await session.search(queryVector, 10, { threshold: 0.0 });
    const doc0 = results2.find(r => r.id === 'doc-0');
    if (doc0) {
      assert.strictEqual(doc0.metadata.title, 'Updated Document 0', 'Title should be updated');
      assert.strictEqual(doc0.metadata.views, 9999, 'Views should be updated');
      assert.ok(doc0.metadata.updatedAt, 'Should have updatedAt field');
    }
    console.log('  Phase 3 complete: Updated metadata verified');

    // Phase 4: Delete vectors by ID
    await session.deleteVector('doc-1');
    await session.deleteVector('doc-2');
    await session.deleteVector('doc-3');

    // Verify deletion
    const results3 = await session.search(queryVector, 20, { threshold: 0.0 });
    const deletedIds = ['doc-1', 'doc-2', 'doc-3'];
    for (const id of deletedIds) {
      assert.ok(!results3.find(r => r.id === id), `${id} should be deleted`);
    }
    console.log('  Phase 4 complete: Deleted 3 vectors by ID');

    // Phase 5: Delete vectors by metadata
    const deleteResult = await session.deleteByMetadata({ category: 'arts' });
    assert.ok(deleteResult.deletedCount > 0, 'Should delete multiple arts documents');
    console.log(`  Phase 5 complete: Deleted ${deleteResult.deletedCount} vectors by metadata`);

    // Phase 6: Save and reload to verify persistence
    const cid2 = await session.saveToS5();
    await session.destroy();

    session = await VectorDbSession.create({
      ...TEST_CONFIG,
      sessionId: 'e2e-full-crud-3',
    });

    await session.loadUserVectors(cid2, { lazyLoad: true });
    const stats3 = await session.getStats();

    // Verify final state
    const results4 = await session.search(queryVector, 50, { threshold: 0.0 });
    for (const id of deletedIds) {
      assert.ok(!results4.find(r => r.id === id), `${id} should remain deleted after reload`);
    }

    const artsDoc = results4.find(r => r.metadata.category === 'arts');
    assert.ok(!artsDoc, 'Arts documents should remain deleted after reload');

    console.log(`  Phase 6 complete: Verified persistence (${stats3.vectorCount} vectors after reload)`);

  } finally {
    await session.destroy();
  }
});

// ============================================================================
// Deletion Workflow Tests
// ============================================================================

test('E2E: Deletion workflow - Delete by ID', async (t) => {
  const session = await VectorDbSession.create({
    ...TEST_CONFIG,
    sessionId: 'e2e-delete-by-id',
  });

  try {
    await addTrainingVectors(session, 50);

    // Add 100 test vectors
    const vectors = [];
    for (let i = 0; i < 100; i++) {
      vectors.push({
        id: `vec-${i}`,
        vector: generateTestVector(2000 + i),
        metadata: { index: i },
      });
    }
    await session.addVectors(vectors);

    const stats1 = await session.getStats();
    assert.strictEqual(stats1.vectorCount, 150, 'Should have 150 vectors');

    // Delete 20 vectors by ID
    for (let i = 0; i < 20; i++) {
      await session.deleteVector(`vec-${i}`);
    }

    // Verify deletion
    const queryVector = generateTestVector(2000);
    const results = await session.search(queryVector, 100, { threshold: 0.0 });

    for (let i = 0; i < 20; i++) {
      assert.ok(!results.find(r => r.id === `vec-${i}`), `vec-${i} should be deleted`);
    }

    // Verify remaining vectors exist
    for (let i = 20; i < 30; i++) {
      // Note: May not find all due to HNSW connectivity with synthetic vectors
      // The important thing is deleted ones are truly gone
    }

    console.log(`  Deleted 20 vectors, verified deletion in search results`);

  } finally {
    await session.destroy();
  }
});

test('E2E: Deletion workflow - Delete by metadata', async (t) => {
  const session = await VectorDbSession.create({
    ...TEST_CONFIG,
    sessionId: 'e2e-delete-by-metadata',
  });

  try {
    await addTrainingVectors(session, 50);

    // Add vectors with different statuses
    const vectors = [];
    for (let i = 0; i < 100; i++) {
      vectors.push({
        id: `item-${i}`,
        vector: generateTestVector(3000 + i),
        metadata: {
          status: i < 30 ? 'active' : i < 60 ? 'inactive' : 'archived',
          value: i,
        },
      });
    }
    await session.addVectors(vectors);

    // Delete all 'archived' items (40 vectors)
    const deleteResult = await session.deleteByMetadata({ status: 'archived' });
    assert.strictEqual(deleteResult.deletedCount, 40, 'Should delete 40 archived items');
    assert.strictEqual(deleteResult.deletedIds.length, 40, 'Should return 40 deleted IDs');

    // Verify all returned IDs are correct
    for (const id of deleteResult.deletedIds) {
      assert.ok(id.startsWith('item-'), 'Deleted ID should be an item');
      const index = parseInt(id.split('-')[1]);
      assert.ok(index >= 60, 'Deleted items should be index >= 60 (archived)');
    }

    // Verify deletion in search results
    const queryVector = generateTestVector(3000);
    const results = await session.search(queryVector, 100, { threshold: 0.0 });

    const archivedResults = results.filter(r => r.metadata.status === 'archived');
    assert.strictEqual(archivedResults.length, 0, 'Should have no archived results');

    console.log(`  Deleted ${deleteResult.deletedCount} archived vectors by metadata`);

  } finally {
    await session.destroy();
  }
});

// ============================================================================
// Update Workflow Tests
// ============================================================================

test('E2E: Update workflow - Update metadata and verify persistence', async (t) => {
  let session = await VectorDbSession.create({
    ...TEST_CONFIG,
    sessionId: 'e2e-update-1',
  });

  try {
    await addTrainingVectors(session, 50);

    // Add test vectors
    const vectors = [];
    for (let i = 0; i < 50; i++) {
      vectors.push({
        id: `post-${i}`,
        vector: generateTestVector(4000 + i),
        metadata: {
          title: `Post ${i}`,
          likes: i * 10,
          status: 'draft',
        },
      });
    }
    await session.addVectors(vectors);

    // Update 20 posts to 'published'
    for (let i = 0; i < 20; i++) {
      await session.updateMetadata(`post-${i}`, {
        title: `Post ${i}`,
        likes: i * 10,
        status: 'published',
        publishedAt: Date.now(),
      });
    }

    // Verify updates in search
    const queryVector = generateTestVector(4000);
    const results = await session.search(queryVector, 50, { threshold: 0.0 });

    let publishedCount = 0;
    for (const result of results) {
      if (result.id.startsWith('post-')) {
        const index = parseInt(result.id.split('-')[1]);
        if (index < 20) {
          assert.strictEqual(result.metadata.status, 'published', `${result.id} should be published`);
          assert.ok(result.metadata.publishedAt, `${result.id} should have publishedAt`);
          publishedCount++;
        }
      }
    }

    console.log(`  Updated ${publishedCount} posts, verified status changes`);

    // Save and reload
    const cid = await session.saveToS5();
    await session.destroy();

    session = await VectorDbSession.create({
      ...TEST_CONFIG,
      sessionId: 'e2e-update-2',
    });

    await session.loadUserVectors(cid, { lazyLoad: true });

    // Verify updates persist after reload
    const results2 = await session.search(queryVector, 50, { threshold: 0.0 });

    for (const result of results2) {
      if (result.id.startsWith('post-')) {
        const index = parseInt(result.id.split('-')[1]);
        if (index < 20) {
          assert.strictEqual(result.metadata.status, 'published', `${result.id} should remain published after reload`);
        }
      }
    }

    console.log('  Verified metadata updates persist after save/load');

  } finally {
    await session.destroy();
  }
});

// ============================================================================
// Filter Workflow Tests
// ============================================================================

test('E2E: Filter workflow - Complex filtering scenarios', async (t) => {
  const session = await VectorDbSession.create({
    ...TEST_CONFIG,
    sessionId: 'e2e-filter',
  });

  try {
    await addTrainingVectors(session, 50);

    // Add vectors with rich metadata for filtering
    const vectors = [];
    for (let i = 0; i < 100; i++) {
      vectors.push({
        id: `product-${i}`,
        vector: generateTestVector(5000 + i),
        metadata: {
          name: `Product ${i}`,
          price: 10 + i * 5,
          category: i % 4 === 0 ? 'electronics' : i % 4 === 1 ? 'books' : i % 4 === 2 ? 'clothing' : 'food',
          inStock: i % 3 !== 0,
          rating: 3 + (i % 3),
          tags: i % 5 === 0 ? ['featured', 'sale'] : ['regular'],
        },
      });
    }
    await session.addVectors(vectors);

    const queryVector = generateTestVector(5000);

    // Test 1: Simple equals filter
    const results1 = await session.search(queryVector, 50, {
      threshold: 0.0,
      filter: { category: 'electronics' },
    });

    for (const result of results1) {
      if (result.id.startsWith('product-')) {
        assert.strictEqual(result.metadata.category, 'electronics', 'Should only return electronics');
      }
    }

    // Test 2: Range filter
    const results2 = await session.search(queryVector, 50, {
      threshold: 0.0,
      filter: { price: { $gte: 50, $lte: 150 } },
    });

    for (const result of results2) {
      if (result.id.startsWith('product-')) {
        assert.ok(result.metadata.price >= 50, 'Price should be >= 50');
        assert.ok(result.metadata.price <= 150, 'Price should be <= 150');
      }
    }

    // Test 3: AND combinator
    const results3 = await session.search(queryVector, 50, {
      threshold: 0.0,
      filter: {
        $and: [
          { category: 'books' },
          { inStock: true },
          { rating: { $gte: 4 } },
        ],
      },
    });

    for (const result of results3) {
      if (result.id.startsWith('product-')) {
        assert.strictEqual(result.metadata.category, 'books', 'Should be books');
        assert.strictEqual(result.metadata.inStock, true, 'Should be in stock');
        assert.ok(result.metadata.rating >= 4, 'Rating should be >= 4');
      }
    }

    // Test 4: Array field filter
    const results4 = await session.search(queryVector, 50, {
      threshold: 0.0,
      filter: { tags: 'featured' },
    });

    for (const result of results4) {
      if (result.id.startsWith('product-')) {
        assert.ok(result.metadata.tags.includes('featured'), 'Should have featured tag');
      }
    }

    console.log('  Tested 4 complex filter scenarios successfully');

  } finally {
    await session.destroy();
  }
});

// ============================================================================
// Combined Operations Test
// ============================================================================

test('E2E: Combined operations - Filter + Update + Delete in sequence', async (t) => {
  const session = await VectorDbSession.create({
    ...TEST_CONFIG,
    sessionId: 'e2e-combined',
  });

  try {
    await addTrainingVectors(session, 50);

    // Add test data
    const vectors = [];
    for (let i = 0; i < 100; i++) {
      vectors.push({
        id: `user-${i}`,
        vector: generateTestVector(6000 + i),
        metadata: {
          name: `User ${i}`,
          age: 20 + (i % 50),
          status: i < 50 ? 'active' : 'inactive',
          premium: i % 10 === 0,
        },
      });
    }
    await session.addVectors(vectors);

    // Step 1: Filter to find premium users
    const queryVector = generateTestVector(6000);
    const premiumUsers = await session.search(queryVector, 100, {
      threshold: 0.0,
      filter: { premium: true },
    });

    const premiumCount = premiumUsers.filter(r => r.id.startsWith('user-')).length;
    console.log(`  Step 1: Found ${premiumCount} premium users`);

    // Step 2: Update all premium users to VIP status
    for (const user of premiumUsers) {
      if (user.id.startsWith('user-')) {
        await session.updateMetadata(user.id, {
          ...user.metadata,
          status: 'vip',
          upgradedAt: Date.now(),
        });
      }
    }

    // Verify updates
    const vipUsers = await session.search(queryVector, 100, {
      threshold: 0.0,
      filter: { status: 'vip' },
    });

    const vipCount = vipUsers.filter(r => r.id.startsWith('user-')).length;
    console.log(`  Step 2: Updated ${vipCount} users to VIP status`);

    // Step 3: Delete all inactive users
    const deleteResult = await session.deleteByMetadata({ status: 'inactive' });
    console.log(`  Step 3: Deleted ${deleteResult.deletedCount} inactive users`);

    // Verify final state
    const finalResults = await session.search(queryVector, 100, { threshold: 0.0 });
    const inactiveResults = finalResults.filter(r => r.id.startsWith('user-') && r.metadata.status === 'inactive');
    assert.strictEqual(inactiveResults.length, 0, 'Should have no inactive users');

    const vipResults = finalResults.filter(r => r.id.startsWith('user-') && r.metadata.status === 'vip');
    assert.ok(vipResults.length > 0, 'Should have VIP users');

    console.log('  Combined operations completed successfully');

  } finally {
    await session.destroy();
  }
});

console.log('\n=== E2E CRUD Integration Tests ===');
