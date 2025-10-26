// NOTE: These integration tests require S5 storage to be available
// at the configured portal URL (http://localhost:5522).
// If S5 is not running, these tests will fail with connection errors.
// This is expected behavior - integration tests verify real S5 operations.

const { describe, test, before, after } = require('node:test');
const assert = require('node:assert');
const { VectorDbSession } = require('../index.js');
const { startS5Service } = require('./helpers/s5-service.cjs');

// S5 service instance (started before all tests)
let s5Service = null;

// Test configuration
const createTestConfig = (sessionId) => ({
  s5Portal: 'http://localhost:5522',
  userSeedPhrase: 'integration-test-seed-phrase-0123456789abcdef',
  sessionId: sessionId,
});

// Helper to create test vectors
function createTestVectors(count, startId = 0) {
  const vectors = [];
  for (let i = 0; i < count; i++) {
    const id = `vec-${startId + i}`;
    // Create diverse vectors to ensure good separation in search results
    const vector = Array(128).fill(0).map((_, idx) => Math.sin((idx + i) * 0.1) * 0.5);
    const metadata = {
      title: `Test Vector ${startId + i}`,
      views: 100 + i,
      tags: ['test', `tag-${i}`],
      nested: { id: startId + i, active: true }
    };
    vectors.push({ id, vector, metadata });
  }
  return vectors;
}

// Helper to compare search results (with tolerance for floating-point differences)
function assertSimilarResults(results1, results2, tolerance = 0.01) {
  assert.strictEqual(results1.length, results2.length, 'Result counts should match');

  for (let i = 0; i < results1.length; i++) {
    const r1 = results1[i];
    const r2 = results2[i];

    // Scores may differ slightly due to floating-point arithmetic
    const scoreDiff = Math.abs(r1.score - r2.score);
    assert.ok(
      scoreDiff < tolerance,
      `Score difference ${scoreDiff} should be less than tolerance ${tolerance} for result ${i}`
    );

    // Metadata should be deeply equal
    assert.deepStrictEqual(
      r1.metadata,
      r2.metadata,
      `Metadata should match for result ${i}`
    );
  }
}

// Start S5 service before all tests
before(async () => {
  console.log('Starting S5 service for integration tests...');
  s5Service = await startS5Service({ port: 5522, mode: 'mock' });
});

// Stop S5 service after all tests
after(async () => {
  if (s5Service) {
    console.log('Stopping S5 service...');
    await s5Service.close();
  }
});

describe('S5 Integration Tests', () => {
  describe('Save to S5', () => {
    test('should save index and return CID', async () => {
      const session = await VectorDbSession.create(createTestConfig('save-test-001'));

      try {
        // Add some vectors
        const vectors = createTestVectors(5);
        await session.addVectors(vectors);

        // Verify vectors were added
        const stats = session.getStats();
        assert.strictEqual(stats.vectorCount, 5, 'Should have 5 vectors before save');

        // Save to S5
        const cid = await session.saveToS5();

        // Verify CID is returned
        assert.ok(cid, 'CID should be returned');
        assert.strictEqual(typeof cid, 'string', 'CID should be a string');
        assert.ok(cid.length > 0, 'CID should not be empty');

        // CID should be the session ID (path identifier)
        assert.strictEqual(cid, 'save-test-001', 'CID should match session ID');
      } finally {
        await session.destroy();
      }
    });

    test('should save empty index', async () => {
      const session = await VectorDbSession.create(createTestConfig('save-empty-test'));

      try {
        // Don't add any vectors
        const stats = session.getStats();
        assert.strictEqual(stats.vectorCount, 0, 'Should have 0 vectors');

        // Save empty index
        const cid = await session.saveToS5();

        // Should succeed
        assert.ok(cid, 'CID should be returned for empty index');
        assert.strictEqual(cid, 'save-empty-test', 'CID should match session ID');
      } finally {
        await session.destroy();
      }
    });
  });

  describe('Load from S5', () => {
    test('should load vectors from CID', async () => {
      // First, create and save an index
      const session1 = await VectorDbSession.create(createTestConfig('load-test-001'));
      let savedCid;

      try {
        const vectors = createTestVectors(7);
        await session1.addVectors(vectors);

        savedCid = await session1.saveToS5();
        assert.ok(savedCid, 'Save should return CID');
      } finally {
        await session1.destroy();
      }

      // Now, load in a new session
      const session2 = await VectorDbSession.create(createTestConfig('load-test-002'));

      try {
        // Load from the saved CID
        await session2.loadUserVectors(savedCid);

        // Verify vectors were loaded
        const stats = session2.getStats();
        assert.strictEqual(stats.vectorCount, 7, 'Should have loaded 7 vectors');
      } finally {
        await session2.destroy();
      }
    });

    test('should throw on invalid CID', async () => {
      const session = await VectorDbSession.create(createTestConfig('load-invalid-test'));

      try {
        // Try to load from non-existent CID
        await assert.rejects(
          async () => await session.loadUserVectors('non-existent-cid-12345'),
          (error) => {
            // Should throw error about missing components or storage error
            assert.ok(
              error.message.includes('Missing') ||
              error.message.includes('not found') ||
              error.message.includes('Failed to load'),
              'Should throw error about missing data'
            );
            return true;
          },
          'Should throw error for invalid CID'
        );
      } finally {
        await session.destroy();
      }
    });
  });

  describe('Round-Trip Persistence', () => {
    test('should preserve vector count', async () => {
      const sessionId = 'roundtrip-count-test';

      // Session 1: Add vectors and save
      const session1 = await VectorDbSession.create(createTestConfig(sessionId));
      let savedCid;

      try {
        const vectors = createTestVectors(10);
        await session1.addVectors(vectors);

        const stats1 = session1.getStats();
        assert.strictEqual(stats1.vectorCount, 10, 'Should have 10 vectors before save');

        savedCid = await session1.saveToS5();
      } finally {
        await session1.destroy();
      }

      // Session 2: Load and verify count
      const session2 = await VectorDbSession.create(createTestConfig('roundtrip-count-verify'));

      try {
        await session2.loadUserVectors(savedCid);

        const stats2 = session2.getStats();
        assert.strictEqual(stats2.vectorCount, 10, 'Should have 10 vectors after load');
      } finally {
        await session2.destroy();
      }
    });

    test('should preserve metadata', async () => {
      const sessionId = 'roundtrip-metadata-test';

      // Session 1: Add vectors with rich metadata and save
      const session1 = await VectorDbSession.create(createTestConfig(sessionId));
      let savedCid;
      const originalVectors = createTestVectors(5);

      try {
        await session1.addVectors(originalVectors);
        savedCid = await session1.saveToS5();
      } finally {
        await session1.destroy();
      }

      // Session 2: Load and verify metadata is preserved
      const session2 = await VectorDbSession.create(createTestConfig('roundtrip-metadata-verify'));

      try {
        await session2.loadUserVectors(savedCid);

        // Search for all vectors
        const queryVector = originalVectors[0].vector;
        const results = await session2.search(queryVector, 5, { threshold: 0.0 });

        // Verify we got results
        assert.ok(results.length > 0, 'Should have search results');

        // Verify metadata structure is preserved
        for (const result of results) {
          assert.strictEqual(typeof result.metadata, 'object', 'Metadata should be object');
          assert.ok(result.metadata.title, 'Should have title');
          assert.strictEqual(typeof result.metadata.views, 'number', 'Should have numeric views');
          assert.ok(Array.isArray(result.metadata.tags), 'Should have tags array');
          assert.ok(result.metadata.nested, 'Should have nested object');
          assert.strictEqual(typeof result.metadata.nested.id, 'number', 'Nested should have id');
          assert.strictEqual(typeof result.metadata.nested.active, 'boolean', 'Nested should have active');
        }

        // Verify specific metadata values for first result
        const firstResult = results[0];
        assert.ok(firstResult.metadata.title.startsWith('Test Vector'), 'Title should match pattern');
        assert.ok(firstResult.metadata.views >= 100, 'Views should be >= 100');
        assert.strictEqual(firstResult.metadata.tags[0], 'test', 'First tag should be "test"');
      } finally {
        await session2.destroy();
      }
    });

    test('should preserve search results', async () => {
      const sessionId = 'roundtrip-search-test';

      // Session 1: Add vectors, search, save
      const session1 = await VectorDbSession.create(createTestConfig(sessionId));
      let savedCid;
      const originalVectors = createTestVectors(8);
      let searchResults1;

      try {
        await session1.addVectors(originalVectors);

        // Search before saving
        const queryVector = originalVectors[0].vector;
        searchResults1 = await session1.search(queryVector, 5);

        assert.ok(searchResults1.length > 0, 'Should have search results before save');

        savedCid = await session1.saveToS5();
      } finally {
        await session1.destroy();
      }

      // Session 2: Load and search with same query
      const session2 = await VectorDbSession.create(createTestConfig('roundtrip-search-verify'));

      try {
        await session2.loadUserVectors(savedCid);

        // Search with same query after loading
        const queryVector = originalVectors[0].vector;
        const searchResults2 = await session2.search(queryVector, 5);

        assert.ok(searchResults2.length > 0, 'Should have search results after load');

        // Results should be similar (same vectors, similar scores)
        // Note: VectorIds might differ due to hashing, so we compare by count and scores
        assert.strictEqual(
          searchResults1.length,
          searchResults2.length,
          'Should have same number of results'
        );

        // Verify score ranges are similar
        const scores1 = searchResults1.map(r => r.score).sort((a, b) => b - a);
        const scores2 = searchResults2.map(r => r.score).sort((a, b) => b - a);

        for (let i = 0; i < scores1.length; i++) {
          const scoreDiff = Math.abs(scores1[i] - scores2[i]);
          assert.ok(
            scoreDiff < 0.01,
            `Score ${i} difference ${scoreDiff} should be < 0.01`
          );
        }
      } finally {
        await session2.destroy();
      }
    });
  });

  describe('Multi-Session', () => {
    test('should allow multiple sessions to load same CID', async () => {
      const saveSessionId = 'multi-session-save';

      // Session 1: Create and save data
      const session1 = await VectorDbSession.create(createTestConfig(saveSessionId));
      let savedCid;

      try {
        const vectors = createTestVectors(6);
        await session1.addVectors(vectors);

        savedCid = await session1.saveToS5();
        assert.ok(savedCid, 'Should save successfully');
      } finally {
        await session1.destroy();
      }

      // Session 2: Load the same CID
      const session2 = await VectorDbSession.create(createTestConfig('multi-session-load-2'));
      let stats2;

      try {
        await session2.loadUserVectors(savedCid);
        stats2 = session2.getStats();
        assert.strictEqual(stats2.vectorCount, 6, 'Session 2 should have 6 vectors');
      } finally {
        await session2.destroy();
      }

      // Session 3: Also load the same CID
      const session3 = await VectorDbSession.create(createTestConfig('multi-session-load-3'));
      let stats3;

      try {
        await session3.loadUserVectors(savedCid);
        stats3 = session3.getStats();
        assert.strictEqual(stats3.vectorCount, 6, 'Session 3 should have 6 vectors');
      } finally {
        await session3.destroy();
      }

      // Session 4: Yet another session loading the same CID
      const session4 = await VectorDbSession.create(createTestConfig('multi-session-load-4'));

      try {
        await session4.loadUserVectors(savedCid);

        // All sessions should have the same data
        const stats4 = session4.getStats();
        assert.strictEqual(stats4.vectorCount, 6, 'Session 4 should have 6 vectors');

        // Verify search works in the last session
        const queryVector = createTestVectors(1)[0].vector;
        const results = await session4.search(queryVector, 3);
        assert.ok(results.length > 0, 'Should be able to search in loaded session');
      } finally {
        await session4.destroy();
      }
    });
  });
});
