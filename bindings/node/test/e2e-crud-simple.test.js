/**
 * Simplified E2E CRUD Integration Tests
 * Focused on verifying core CRUD workflows work end-to-end
 */

const { test, before, after } = require('node:test');
const assert = require('node:assert');
const { VectorDbSession } = require('../index.js');
const { startS5Service } = require('./helpers/s5-service.cjs');

// S5 service instance
let s5Service = null;

const TEST_CONFIG = {
  s5Portal: 'http://localhost:5522',
  userSeedPhrase: 'test seed phrase for e2e crud testing only',
  sessionId: 'e2e-crud-simple',
  debug: false,
};

const VECTOR_DIM = 384;

function generateTestVector(seed) {
  const vector = new Array(VECTOR_DIM);
  for (let i = 0; i < VECTOR_DIM; i++) {
    vector[i] = Math.sin(seed * i * 0.01) * 0.5 + 0.5;
  }
  return vector;
}

async function addTrainingVectors(session, count = 50) {
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

// Start S5 service
before(async () => {
  console.log('Starting S5 service for E2E CRUD tests...');
  s5Service = await startS5Service({ port: 5522, mode: 'mock' });
});

// Stop S5 service
after(async () => {
  console.log('Stopping S5 service...');
  if (s5Service && s5Service.stop) {
    s5Service.stop();
  }
  console.log('✓ S5 service stopped');
});

// ============================================================================
// Test 1: Full CRUD Workflow
// ============================================================================

test('E2E: Create → Add → Save → Load → Update → Delete → Save → Load', async (t) => {
  // Phase 1: Create and add vectors
  let session = await VectorDbSession.create({
    ...TEST_CONFIG,
    sessionId: 'e2e-crud-1',
  });

  try {
    await addTrainingVectors(session);

    // Add 20 test vectors
    const testVectors = [];
    for (let i = 0; i < 20; i++) {
      testVectors.push({
        id: `doc-${i}`,
        vector: generateTestVector(1000 + i),
        metadata: {
          title: `Document ${i}`,
          category: i % 2 === 0 ? 'tech' : 'science',
          views: 100 + i * 10,
        },
      });
    }
    await session.addVectors(testVectors);

    const stats1 = await session.getStats();
    assert.strictEqual(stats1.vectorCount, 70, 'Should have 70 vectors');

    // Save to S5
    const cid1 = await session.saveToS5();
    assert.ok(cid1, 'Should return CID');
    console.log(`  ✓ Phase 1: Saved 70 vectors (CID: ${cid1})`);

    await session.destroy();

    // Phase 2: Load and verify
    session = await VectorDbSession.create({
      ...TEST_CONFIG,
      sessionId: 'e2e-crud-2',
    });

    await session.loadUserVectors(cid1, { lazyLoad: true });
    const stats2 = await session.getStats();
    assert.strictEqual(stats2.vectorCount, 70, 'Should load 70 vectors');

    const queryVector = generateTestVector(1000);
    const results1 = await session.search(queryVector, 10, { threshold: 0.0 });
    assert.ok(results1.length > 0, 'Should find vectors');
    console.log(`  ✓ Phase 2: Loaded and verified vectors`);

    // Phase 3: Update metadata
    await session.updateMetadata('doc-0', {
      title: 'Updated Document 0',
      category: 'tech',
      views: 9999,
      updated: true,
    });

    const results2 = await session.search(queryVector, 10, { threshold: 0.0 });
    const doc0 = results2.find(r => r.id === 'doc-0');
    if (doc0) {
      assert.strictEqual(doc0.metadata.title, 'Updated Document 0');
      assert.strictEqual(doc0.metadata.views, 9999);
      assert.strictEqual(doc0.metadata.updated, true);
    }
    console.log('  ✓ Phase 3: Updated metadata');

    // Phase 4: Delete vectors
    await session.deleteVector('doc-1');
    await session.deleteVector('doc-2');

    const results3 = await session.search(queryVector, 20, { threshold: 0.0 });
    assert.ok(!results3.find(r => r.id === 'doc-1'), 'doc-1 should be deleted');
    assert.ok(!results3.find(r => r.id === 'doc-2'), 'doc-2 should be deleted');
    console.log('  ✓ Phase 4: Deleted 2 vectors');

    // Phase 5: Delete by metadata
    const deleteResult = await session.deleteByMetadata({ category: 'science' });
    assert.ok(deleteResult.deletedCount > 0, 'Should delete science docs');
    console.log(`  ✓ Phase 5: Deleted ${deleteResult.deletedCount} vectors by metadata`);

    // Phase 6: Save and reload
    const cid2 = await session.saveToS5();
    await session.destroy();

    session = await VectorDbSession.create({
      ...TEST_CONFIG,
      sessionId: 'e2e-crud-3',
    });

    await session.loadUserVectors(cid2, { lazyLoad: true });

    const results4 = await session.search(queryVector, 30, { threshold: 0.0 });
    assert.ok(!results4.find(r => r.id === 'doc-1'), 'doc-1 should remain deleted');
    assert.ok(!results4.find(r => r.id === 'doc-2'), 'doc-2 should remain deleted');
    const scienceDocs = results4.filter(r => r.id.startsWith('doc-') && r.metadata.category === 'science');
    assert.strictEqual(scienceDocs.length, 0, 'Science docs should remain deleted');

    console.log('  ✓ Phase 6: Verified persistence after reload');

  } finally {
    await session.destroy();
  }
});

// ============================================================================
// Test 2: Filtered Search
// ============================================================================

test('E2E: Filtered search with complex queries', async (t) => {
  const session = await VectorDbSession.create({
    ...TEST_CONFIG,
    sessionId: 'e2e-filter',
  });

  try {
    await addTrainingVectors(session);

    // Add products
    const products = [];
    for (let i = 0; i < 30; i++) {
      products.push({
        id: `product-${i}`,
        vector: generateTestVector(2000 + i),
        metadata: {
          name: `Product ${i}`,
          price: 10 + i * 5,
          category: i % 3 === 0 ? 'electronics' : i % 3 === 1 ? 'books' : 'clothing',
          inStock: i % 4 !== 0,
        },
      });
    }
    await session.addVectors(products);

    const queryVector = generateTestVector(2000);

    // Test Equals filter
    const results1 = await session.search(queryVector, 20, {
      threshold: 0.0,
      filter: { category: 'electronics' },
    });
    for (const result of results1) {
      if (result.id.startsWith('product-')) {
        assert.strictEqual(result.metadata.category, 'electronics');
      }
    }
    console.log('  ✓ Equals filter works');

    // Test Range filter
    const results2 = await session.search(queryVector, 20, {
      threshold: 0.0,
      filter: { price: { $gte: 50, $lte: 100 } },
    });
    for (const result of results2) {
      if (result.id.startsWith('product-')) {
        assert.ok(result.metadata.price >= 50 && result.metadata.price <= 100);
      }
    }
    console.log('  ✓ Range filter works');

    // Test AND combinator
    const results3 = await session.search(queryVector, 20, {
      threshold: 0.0,
      filter: {
        $and: [
          { category: 'books' },
          { inStock: true },
        ],
      },
    });
    for (const result of results3) {
      if (result.id.startsWith('product-')) {
        assert.strictEqual(result.metadata.category, 'books');
        assert.strictEqual(result.metadata.inStock, true);
      }
    }
    console.log('  ✓ AND combinator works');

  } finally {
    await session.destroy();
  }
});

// ============================================================================
// Test 3: Combined Operations
// ============================================================================

test('E2E: Combined filter + update + delete operations', async (t) => {
  const session = await VectorDbSession.create({
    ...TEST_CONFIG,
    sessionId: 'e2e-combined',
  });

  try {
    await addTrainingVectors(session);

    // Add users
    const users = [];
    for (let i = 0; i < 30; i++) {
      users.push({
        id: `user-${i}`,
        vector: generateTestVector(3000 + i),
        metadata: {
          name: `User ${i}`,
          status: i < 15 ? 'active' : 'inactive',
          premium: i % 10 === 0,
        },
      });
    }
    await session.addVectors(users);

    const queryVector = generateTestVector(3000);

    // Step 1: Find premium users
    const premiumUsers = await session.search(queryVector, 30, {
      threshold: 0.0,
      filter: { premium: true },
    });
    const premiumCount = premiumUsers.filter(r => r.id.startsWith('user-')).length;
    console.log(`  Step 1: Found ${premiumCount} premium users`);

    // Step 2: Update premium users
    for (const user of premiumUsers) {
      if (user.id.startsWith('user-')) {
        await session.updateMetadata(user.id, {
          ...user.metadata,
          status: 'vip',
        });
      }
    }

    // Verify updates
    const vipUsers = await session.search(queryVector, 30, {
      threshold: 0.0,
      filter: { status: 'vip' },
    });
    const vipCount = vipUsers.filter(r => r.id.startsWith('user-')).length;
    console.log(`  Step 2: Updated ${vipCount} users to VIP`);

    // Step 3: Delete inactive users
    const deleteResult = await session.deleteByMetadata({ status: 'inactive' });
    console.log(`  Step 3: Deleted ${deleteResult.deletedCount} inactive users`);

    // Verify final state
    const finalResults = await session.search(queryVector, 30, { threshold: 0.0 });
    const inactiveResults = finalResults.filter(
      r => r.id.startsWith('user-') && r.metadata.status === 'inactive'
    );
    assert.strictEqual(inactiveResults.length, 0, 'No inactive users should remain');

    console.log('  ✓ Combined operations completed successfully');

  } finally {
    await session.destroy();
  }
});

console.log('\n=== E2E CRUD Integration Tests (Simplified) ===');
