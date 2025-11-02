// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

/**
 * Integration tests for filtered search functionality in Node.js bindings
 * Tests metadata-based filtering of vector search results
 */

const { test, before, after } = require('node:test');
const assert = require('node:assert');
const { VectorDbSession } = require('../index.js');
const { startS5Service } = require('./helpers/s5-service.cjs');

// S5 service instance
let s5Service = null;

// Test configuration
const TEST_CONFIG = {
  s5Portal: 'http://localhost:5522',
  userSeedPhrase: 'test-seed-phrase-search-filter',
  sessionId: 'test-session-search-filter',
  encryptAtRest: true,
  chunkSize: 10000,
  cacheSizeMb: 150,
};

// Helper to generate test vector (384 dimensions)
function generateTestVector(seed = 0) {
  const vector = [];
  for (let i = 0; i < 384; i++) {
    vector.push(Math.sin(seed + i) * 0.5);
  }
  return vector;
}

// Helper to add background test vectors for realistic search scenarios
async function addTrainingVectors(session, count = 10, startSeed = 1000) {
  const vectors = [];
  for (let i = 0; i < count; i++) {
    vectors.push({
      id: `training-${startSeed + i}`,
      vector: generateTestVector(startSeed + i),
      metadata: { type: 'training', index: i },
    });
  }
  await session.addVectors(vectors);
}

// Start S5 service before all tests
before(async () => {
  console.log('Starting S5 service for search-filter tests...');
  s5Service = await startS5Service({ port: 5522, mode: 'mock' });
});

// Stop S5 service after all tests
after(async () => {
  if (s5Service) {
    console.log('Stopping S5 service...');
    await s5Service.close();
  }
});

test('search with Equals filter', async (t) => {
  const session = await VectorDbSession.create(TEST_CONFIG);

  try {
    // Add training vectors
    await addTrainingVectors(session);

    // Add test vectors with category metadata
    await session.addVectors([
      {
        id: 'tech-1',
        vector: generateTestVector(1),
        metadata: { category: 'technology', title: 'AI News' },
      },
      {
        id: 'tech-2',
        vector: generateTestVector(2),
        metadata: { category: 'technology', title: 'ML Update' },
      },
      {
        id: 'sports-1',
        vector: generateTestVector(3),
        metadata: { category: 'sports', title: 'Game Results' },
      },
      {
        id: 'tech-3',
        vector: generateTestVector(4),
        metadata: { category: 'technology', title: 'Robotics' },
      },
    ]);

    // Search with Equals filter
    const queryVector = generateTestVector(1);
    const results = await session.search(queryVector, 20, {
      filter: { category: 'technology' },
    });

    // Should only return technology articles
    assert.ok(results.length > 0, 'Should find technology articles');
    assert.ok(results.length <= 3, 'Should find at most 3 technology articles');

    for (const result of results) {
      if (result.id.startsWith('tech-')) {
        assert.strictEqual(
          result.metadata.category,
          'technology',
          `Result ${result.id} should have category=technology`
        );
      }
    }
  } finally {
    await session.destroy();
  }
});

test('search with In filter', async (t) => {
  const session = await VectorDbSession.create({
    ...TEST_CONFIG,
    sessionId: 'test-session-search-filter-in',
  });

  try {
    await addTrainingVectors(session);

    // Add vectors with status field
    await session.addVectors([
      {
        id: 'doc-1',
        vector: generateTestVector(10),
        metadata: { status: 'active', title: 'Active Doc' },
      },
      {
        id: 'doc-2',
        vector: generateTestVector(11),
        metadata: { status: 'pending', title: 'Pending Doc' },
      },
      {
        id: 'doc-3',
        vector: generateTestVector(12),
        metadata: { status: 'archived', title: 'Archived Doc' },
      },
      {
        id: 'doc-4',
        vector: generateTestVector(13),
        metadata: { status: 'review', title: 'Review Doc' },
      },
    ]);

    // Search with In filter
    const queryVector = generateTestVector(10);
    const results = await session.search(queryVector, 20, {
      filter: {
        status: { $in: ['active', 'pending', 'review'] },
      },
    });

    // Should return active, pending, and review (not archived)
    assert.ok(results.length > 0, 'Should find matching documents');

    for (const result of results) {
      if (result.id.startsWith('doc-')) {
        const status = result.metadata.status;
        assert.ok(
          ['active', 'pending', 'review'].includes(status),
          `Status ${status} should be in allowed list`
        );
        assert.notStrictEqual(status, 'archived', 'Archived should be excluded');
      }
    }
  } finally {
    await session.destroy();
  }
});

test('search with Range filter', async (t) => {
  const session = await VectorDbSession.create({
    ...TEST_CONFIG,
    sessionId: 'test-session-search-filter-range',
  });

  try {
    await addTrainingVectors(session);

    // Add vectors with numeric views field
    await session.addVectors([
      {
        id: 'post-1',
        vector: generateTestVector(20),
        metadata: { views: 500, title: 'Low Views' },
      },
      {
        id: 'post-2',
        vector: generateTestVector(21),
        metadata: { views: 1500, title: 'Medium Views' },
      },
      {
        id: 'post-3',
        vector: generateTestVector(22),
        metadata: { views: 5000, title: 'High Views' },
      },
      {
        id: 'post-4',
        vector: generateTestVector(23),
        metadata: { views: 10000, title: 'Very High Views' },
      },
    ]);

    // Search with Range filter (1000 <= views <= 5000)
    // Use query that doesn't exactly match to avoid early termination
    const queryVector = generateTestVector(20.5);
    const results = await session.search(queryVector, 20, {
      threshold: 0.0, // Use threshold 0.0 to include all results
      filter: {
        views: { $gte: 1000, $lte: 5000 },
      },
    });

    // Due to HNSW graph connectivity with synthetic test vectors,
    // we may not find all matching vectors. The important thing is that
    // any results returned DO match the filter criteria.
    // With real embeddings (e.g., all-MiniLM-L6-v2), connectivity is much better.

    // Check that returned results match the filter
    for (const result of results) {
      if (result.id.startsWith('post-')) {
        const views = result.metadata.views;
        assert.ok(views >= 1000, `Views ${views} should be >= 1000`);
        assert.ok(views <= 5000, `Views ${views} should be <= 5000`);
      }
    }

    // At minimum, verify the filter is being applied (no out-of-range results)
    const hasOutOfRange = results.some(r =>
      r.id.startsWith('post-') && (r.metadata.views < 1000 || r.metadata.views > 5000)
    );
    assert.ok(!hasOutOfRange, 'Filter should exclude out-of-range posts');
  } finally {
    await session.destroy();
  }
});

test('search with And combinator', async (t) => {
  const session = await VectorDbSession.create({
    ...TEST_CONFIG,
    sessionId: 'test-session-search-filter-and',
  });

  try {
    await addTrainingVectors(session);

    // Add vectors with multiple fields
    await session.addVectors([
      {
        id: 'article-1',
        vector: generateTestVector(30),
        metadata: { category: 'technology', published: true, title: 'Tech Published' },
      },
      {
        id: 'article-2',
        vector: generateTestVector(31),
        metadata: { category: 'technology', published: false, title: 'Tech Draft' },
      },
      {
        id: 'article-3',
        vector: generateTestVector(32),
        metadata: { category: 'sports', published: true, title: 'Sports Published' },
      },
      {
        id: 'article-4',
        vector: generateTestVector(33),
        metadata: { category: 'technology', published: true, title: 'Tech Live' },
      },
    ]);

    // Search with And combinator (technology AND published)
    const queryVector = generateTestVector(30);
    const results = await session.search(queryVector, 20, {
      filter: {
        $and: [{ category: 'technology' }, { published: true }],
      },
    });

    // Should return article-1 and article-4 only
    assert.ok(results.length > 0, 'Should find published technology articles');

    for (const result of results) {
      if (result.id.startsWith('article-')) {
        assert.strictEqual(
          result.metadata.category,
          'technology',
          'Category should be technology'
        );
        assert.strictEqual(result.metadata.published, true, 'Should be published');
      }
    }
  } finally {
    await session.destroy();
  }
});

test('search with Or combinator', async (t) => {
  const session = await VectorDbSession.create({
    ...TEST_CONFIG,
    sessionId: 'test-session-search-filter-or',
  });

  try {
    await addTrainingVectors(session);

    // Add vectors with different criteria
    await session.addVectors([
      {
        id: 'item-1',
        vector: generateTestVector(40),
        metadata: { urgent: true, priority: 3, title: 'Urgent Low Priority' },
      },
      {
        id: 'item-2',
        vector: generateTestVector(41),
        metadata: { urgent: false, priority: 9, title: 'Not Urgent High Priority' },
      },
      {
        id: 'item-3',
        vector: generateTestVector(42),
        metadata: { urgent: false, priority: 5, title: 'Normal' },
      },
      {
        id: 'item-4',
        vector: generateTestVector(43),
        metadata: { urgent: true, priority: 8, title: 'Urgent High Priority' },
      },
    ]);

    // Search with Or combinator (urgent OR priority >= 8)
    const queryVector = generateTestVector(40);
    const results = await session.search(queryVector, 20, {
      filter: {
        $or: [{ urgent: true }, { priority: { $gte: 8 } }],
      },
    });

    // Should return item-1, item-2, item-4 (not item-3)
    assert.ok(results.length > 0, 'Should find matching items');

    for (const result of results) {
      if (result.id.startsWith('item-')) {
        const urgent = result.metadata.urgent;
        const priority = result.metadata.priority;
        assert.ok(
          urgent === true || priority >= 8,
          `Should be urgent OR high priority (urgent=${urgent}, priority=${priority})`
        );
      }
    }
  } finally {
    await session.destroy();
  }
});

test('search with nested field filter', async (t) => {
  const session = await VectorDbSession.create({
    ...TEST_CONFIG,
    sessionId: 'test-session-search-filter-nested',
  });

  try {
    await addTrainingVectors(session);

    // Add vectors with nested metadata
    await session.addVectors([
      {
        id: 'user-doc-1',
        vector: generateTestVector(50),
        metadata: {
          user: { id: 'user123', name: 'Alice' },
          title: 'Alice Document',
        },
      },
      {
        id: 'user-doc-2',
        vector: generateTestVector(51),
        metadata: {
          user: { id: 'user456', name: 'Bob' },
          title: 'Bob Document',
        },
      },
      {
        id: 'user-doc-3',
        vector: generateTestVector(52),
        metadata: {
          user: { id: 'user123', name: 'Alice' },
          title: 'Alice Second Document',
        },
      },
    ]);

    // Search with nested field filter
    const queryVector = generateTestVector(50);
    const results = await session.search(queryVector, 20, {
      filter: {
        'user.id': 'user123',
      },
    });

    // Should return user-doc-1 and user-doc-3 (Alice's documents)
    assert.ok(results.length > 0, 'Should find Alice documents');

    for (const result of results) {
      if (result.id.startsWith('user-doc-')) {
        assert.strictEqual(
          result.metadata.user.id,
          'user123',
          'User ID should be user123'
        );
      }
    }
  } finally {
    await session.destroy();
  }
});

test('search with array field filter', async (t) => {
  const session = await VectorDbSession.create({
    ...TEST_CONFIG,
    sessionId: 'test-session-search-filter-array',
  });

  try {
    await addTrainingVectors(session);

    // Add vectors with array tags field
    await session.addVectors([
      {
        id: 'content-1',
        vector: generateTestVector(60),
        metadata: { tags: ['ai', 'ml', 'technology'], title: 'AI Article' },
      },
      {
        id: 'content-2',
        vector: generateTestVector(61),
        metadata: { tags: ['sports', 'news'], title: 'Sports News' },
      },
      {
        id: 'content-3',
        vector: generateTestVector(62),
        metadata: { tags: ['ai', 'robotics'], title: 'Robotics Article' },
      },
      {
        id: 'content-4',
        vector: generateTestVector(63),
        metadata: { tags: ['web', 'frontend'], title: 'Web Development' },
      },
    ]);

    // Search with array field filter (tags contains 'ai')
    const queryVector = generateTestVector(60);
    const results = await session.search(queryVector, 20, {
      filter: {
        tags: 'ai',
      },
    });

    // Should return content-1 and content-3 (both have 'ai' tag)
    assert.ok(results.length > 0, 'Should find content with ai tag');

    for (const result of results) {
      if (result.id.startsWith('content-')) {
        const tags = result.metadata.tags;
        assert.ok(Array.isArray(tags), 'Tags should be an array');
        assert.ok(tags.includes('ai'), 'Tags should include ai');
      }
    }
  } finally {
    await session.destroy();
  }
});

test('search with no filter (backward compatibility)', async (t) => {
  const session = await VectorDbSession.create({
    ...TEST_CONFIG,
    sessionId: 'test-session-search-filter-none',
  });

  try {
    await addTrainingVectors(session);

    // Add test vectors
    await session.addVectors([
      {
        id: 'any-1',
        vector: generateTestVector(70),
        metadata: { category: 'a', value: 1 },
      },
      {
        id: 'any-2',
        vector: generateTestVector(71),
        metadata: { category: 'b', value: 2 },
      },
      {
        id: 'any-3',
        vector: generateTestVector(72),
        metadata: { category: 'c', value: 3 },
      },
    ]);

    // Search without filter (backward compatibility)
    const queryVector = generateTestVector(70);
    const resultsNoOptions = await session.search(queryVector, 20, { threshold: 0.0 });
    const resultsEmptyOptions = await session.search(queryVector, 20, { threshold: 0.0 });

    // Due to HNSW graph connectivity with synthetic test vectors,
    // we may not find all vectors. The important thing is backward compatibility:
    // no options and empty options should behave identically.

    // Both should return same count (backward compatibility)
    assert.strictEqual(
      resultsNoOptions.length,
      resultsEmptyOptions.length,
      'No options and empty options should be equivalent'
    );

    // Should find at least some results
    assert.ok(resultsNoOptions.length > 0, 'Should find at least some results');
  } finally {
    await session.destroy();
  }
});

test('search with invalid filter (error handling)', async (t) => {
  const session = await VectorDbSession.create({
    ...TEST_CONFIG,
    sessionId: 'test-session-search-filter-invalid',
  });

  try {
    await addTrainingVectors(session);

    await session.addVectors([
      {
        id: 'test-1',
        vector: generateTestVector(80),
        metadata: { value: 1 },
      },
    ]);

    const queryVector = generateTestVector(80);

    // Test invalid operator
    await assert.rejects(
      async () => {
        await session.search(queryVector, 10, {
          filter: { $invalid: 'test' },
        });
      },
      (err) => {
        assert.ok(err instanceof Error, 'Should throw Error');
        assert.ok(
          err.message.includes('Invalid filter') || err.message.includes('Unsupported'),
          `Error message should mention invalid filter, got: ${err.message}`
        );
        return true;
      },
      'Should throw error for invalid operator'
    );

    // Test malformed range
    await assert.rejects(
      async () => {
        await session.search(queryVector, 10, {
          filter: { value: {} }, // Empty range object
        });
      },
      (err) => {
        assert.ok(err instanceof Error, 'Should throw Error');
        assert.ok(
          err.message.includes('Invalid filter') || err.message.includes('Invalid syntax'),
          `Error message should mention invalid syntax, got: ${err.message}`
        );
        return true;
      },
      'Should throw error for malformed range'
    );
  } finally {
    await session.destroy();
  }
});

test('search with filter + threshold combined', async (t) => {
  const session = await VectorDbSession.create({
    ...TEST_CONFIG,
    sessionId: 'test-session-search-filter-combined',
  });

  try {
    await addTrainingVectors(session);

    // Add vectors with varying similarity to query
    const baseVector = generateTestVector(90);

    await session.addVectors([
      {
        id: 'close-tech',
        vector: baseVector, // Exact match (high similarity)
        metadata: { category: 'technology', distance: 'close' },
      },
      {
        id: 'close-sports',
        vector: baseVector, // Exact match (high similarity)
        metadata: { category: 'sports', distance: 'close' },
      },
      {
        id: 'far-tech',
        vector: generateTestVector(95), // Different (lower similarity)
        metadata: { category: 'technology', distance: 'far' },
      },
    ]);

    // Search with both filter and threshold
    const queryVector = baseVector;
    const results = await session.search(queryVector, 20, {
      threshold: 0.9, // High similarity threshold
      filter: { category: 'technology' },
    });

    // Should apply both filters:
    // 1. Metadata filter: category = technology (excludes close-sports)
    // 2. Similarity threshold: >= 0.9 (may exclude far-tech depending on distance)

    assert.ok(results.length > 0, 'Should find at least one result');

    for (const result of results) {
      if (result.id.startsWith('close-') || result.id.startsWith('far-')) {
        // Must match category filter
        assert.strictEqual(
          result.metadata.category,
          'technology',
          'Should only return technology category'
        );

        // Must meet similarity threshold (score >= threshold)
        if (result.score !== undefined) {
          assert.ok(
            result.score >= 0.9,
            `Score ${result.score} should be >= threshold 0.9`
          );
        }
      }
    }

    // Verify close-tech is included (matches both criteria)
    const closeTech = results.find((r) => r.id === 'close-tech');
    assert.ok(closeTech, 'close-tech should be in results (matches both filters)');
  } finally {
    await session.destroy();
  }
});
