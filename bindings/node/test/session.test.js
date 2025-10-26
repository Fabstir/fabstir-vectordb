const { describe, test } = require('node:test');
const assert = require('node:assert');
const { VectorDbSession } = require('../index.js');

// Test configuration
const validConfig = {
  s5Portal: 'http://localhost:5524',
  userSeedPhrase: 'test-seed-phrase-for-unit-tests-12345678901234567890',
  sessionId: 'test-session-unit-tests',
};

// Helper to create test vectors
function createTestVectors(count, startId = 0) {
  const vectors = [];
  for (let i = 0; i < count; i++) {
    const id = `vec-${startId + i}`;
    const vector = Array(128).fill(0).map((_, idx) => Math.sin(idx + i) * 0.5);
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

describe('VectorDBSession', () => {
  describe('Session Creation', () => {
    test('should create session with valid config', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        assert.ok(session, 'Session should be created');

        // Verify session has expected methods
        assert.strictEqual(typeof session.addVectors, 'function');
        assert.strictEqual(typeof session.search, 'function');
        assert.strictEqual(typeof session.saveToS5, 'function');
        assert.strictEqual(typeof session.loadUserVectors, 'function');
        assert.strictEqual(typeof session.getStats, 'function');
        assert.strictEqual(typeof session.destroy, 'function');
      } finally {
        await session.destroy();
      }
    });

    test('should throw on missing s5_portal', async () => {
      const invalidConfig = {
        ...validConfig,
        s5Portal: '',
      };

      await assert.rejects(
        async () => await VectorDbSession.create(invalidConfig),
        (error) => {
          assert.ok(error.message.includes('s5_portal') || error.message.includes('required'));
          return true;
        },
        'Should throw error for missing s5_portal'
      );
    });

    test('should throw on missing user_seed_phrase', async () => {
      const invalidConfig = {
        ...validConfig,
        userSeedPhrase: '',
      };

      await assert.rejects(
        async () => await VectorDbSession.create(invalidConfig),
        (error) => {
          assert.ok(error.message.includes('seed') || error.message.includes('required'));
          return true;
        },
        'Should throw error for missing user_seed_phrase'
      );
    });

    test('should throw on missing session_id', async () => {
      const invalidConfig = {
        ...validConfig,
        sessionId: '',
      };

      await assert.rejects(
        async () => await VectorDbSession.create(invalidConfig),
        (error) => {
          assert.ok(error.message.includes('session') || error.message.includes('required'));
          return true;
        },
        'Should throw error for missing session_id'
      );
    });
  });

  describe('Vector Operations', () => {
    test('should add vectors with object metadata', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        const vectors = createTestVectors(3);

        // Verify metadata is objects, not strings
        assert.strictEqual(typeof vectors[0].metadata, 'object');
        assert.strictEqual(vectors[0].metadata.title, 'Test Vector 0');
        assert.strictEqual(vectors[0].metadata.views, 100);
        assert.deepStrictEqual(vectors[0].metadata.tags, ['test', 'tag-0']);

        // Add vectors with object metadata
        await session.addVectors(vectors);

        // Verify vectors were added
        const stats = session.getStats();
        assert.strictEqual(stats.vectorCount, 3, 'Should have 3 vectors');
      } finally {
        await session.destroy();
      }
    });

    test('should search and return object metadata', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        // Add test vectors
        const vectors = createTestVectors(5);
        await session.addVectors(vectors);

        // Search using first vector as query
        const queryVector = vectors[0].vector;
        const results = await session.search(queryVector, 3);

        // Verify we got results
        assert.ok(results.length > 0, 'Should return search results');
        assert.ok(results.length <= 3, 'Should return at most 3 results');

        // Verify result structure
        const firstResult = results[0];
        assert.ok(firstResult.id, 'Result should have id');
        assert.strictEqual(typeof firstResult.score, 'number', 'Result should have numeric score');
        assert.ok(firstResult.score >= 0 && firstResult.score <= 1, 'Score should be between 0 and 1');

        // Verify metadata is an object (not a string!)
        assert.strictEqual(typeof firstResult.metadata, 'object', 'Metadata should be object');
        assert.ok(firstResult.metadata !== null, 'Metadata should not be null');

        // Verify metadata structure
        assert.ok(firstResult.metadata.title, 'Metadata should have title');
        assert.strictEqual(typeof firstResult.metadata.views, 'number', 'Metadata should have numeric views');
        assert.ok(Array.isArray(firstResult.metadata.tags), 'Metadata should have tags array');
        assert.ok(firstResult.metadata.nested, 'Metadata should have nested object');
        assert.strictEqual(typeof firstResult.metadata.nested.id, 'number', 'Nested metadata should have id');
      } finally {
        await session.destroy();
      }
    });

    test('should handle different metadata types', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        const vectors = [
          {
            id: 'vec-obj',
            vector: Array(128).fill(0.1),
            metadata: { type: 'object', value: 42 }
          },
          {
            id: 'vec-array',
            vector: Array(128).fill(0.2),
            metadata: ['item1', 'item2', 'item3']
          },
          {
            id: 'vec-nested',
            vector: Array(128).fill(0.3),
            metadata: { user: { name: 'alice', age: 30 }, active: true }
          },
          {
            id: 'vec-null',
            vector: Array(128).fill(0.4),
            metadata: null
          },
          {
            id: 'vec-number',
            vector: Array(128).fill(0.5),
            metadata: 123.45
          }
        ];

        await session.addVectors(vectors);

        // Search and verify metadata types are preserved
        const results = await session.search(vectors[0].vector, 5);
        assert.ok(results.length > 0, 'Should return results');

        // Find each vector in results and verify metadata
        const findResult = (id) => results.find(r => r.id === id);

        const objResult = findResult('vec-obj');
        if (objResult) {
          assert.strictEqual(typeof objResult.metadata, 'object');
          assert.strictEqual(objResult.metadata.type, 'object');
        }

        const arrayResult = findResult('vec-array');
        if (arrayResult) {
          assert.ok(Array.isArray(arrayResult.metadata));
        }

        const nestedResult = findResult('vec-nested');
        if (nestedResult) {
          assert.strictEqual(typeof nestedResult.metadata, 'object');
          assert.strictEqual(nestedResult.metadata.user.name, 'alice');
        }
      } finally {
        await session.destroy();
      }
    });
  });

  describe('Statistics', () => {
    test('should return accurate vector counts', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        // Initially should have 0 vectors
        let stats = session.getStats();
        assert.strictEqual(stats.vectorCount, 0, 'Should start with 0 vectors');
        assert.strictEqual(stats.indexType, 'hybrid', 'Should be hybrid index');

        // Add 5 vectors
        const vectors = createTestVectors(5);
        await session.addVectors(vectors);

        // Should now have 5 vectors
        stats = session.getStats();
        assert.strictEqual(stats.vectorCount, 5, 'Should have 5 vectors after adding');
        assert.ok(stats.memoryUsageMb > 0, 'Memory usage should be greater than 0');

        // Verify stats structure
        assert.strictEqual(typeof stats.memoryUsageMb, 'number');
        assert.strictEqual(typeof stats.indexType, 'string');
      } finally {
        await session.destroy();
      }
    });

    test('should track index type correctly', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        const stats = session.getStats();

        assert.strictEqual(stats.indexType, 'hybrid', 'Index type should be hybrid');
        assert.ok(typeof stats.hnswVectorCount === 'number' || stats.hnswVectorCount === undefined);
        assert.ok(typeof stats.ivfVectorCount === 'number' || stats.ivfVectorCount === undefined);
      } finally {
        await session.destroy();
      }
    });

    test('should increment count after multiple additions', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        // Add vectors in batches
        await session.addVectors(createTestVectors(3, 0));
        let stats = session.getStats();
        assert.strictEqual(stats.vectorCount, 3);

        await session.addVectors(createTestVectors(2, 3));
        stats = session.getStats();
        assert.strictEqual(stats.vectorCount, 5);

        await session.addVectors(createTestVectors(5, 5));
        stats = session.getStats();
        assert.strictEqual(stats.vectorCount, 10);
      } finally {
        await session.destroy();
      }
    });
  });

  describe('Session Lifecycle', () => {
    test('should destroy session successfully', async () => {
      const session = await VectorDbSession.create(validConfig);

      // Add some vectors
      await session.addVectors(createTestVectors(3));

      // Verify vectors exist
      const stats = session.getStats();
      assert.strictEqual(stats.vectorCount, 3);

      // Destroy session
      await session.destroy();

      // After destroy, session should be unusable
      // (Next test will verify this)
    });

    test('should throw on operations after destroy - addVectors', async () => {
      const session = await VectorDbSession.create(validConfig);
      await session.destroy();

      await assert.rejects(
        async () => await session.addVectors(createTestVectors(1)),
        (error) => {
          assert.ok(
            error.message.includes('destroyed') || error.message.includes('SESSION'),
            'Should mention session is destroyed'
          );
          return true;
        },
        'addVectors should throw after destroy'
      );
    });

    test('should throw on operations after destroy - search', async () => {
      const session = await VectorDbSession.create(validConfig);
      await session.destroy();

      const queryVector = Array(128).fill(0.5);
      await assert.rejects(
        async () => await session.search(queryVector, 5),
        (error) => {
          assert.ok(
            error.message.includes('destroyed') || error.message.includes('SESSION'),
            'Should mention session is destroyed'
          );
          return true;
        },
        'search should throw after destroy'
      );
    });

    test('should throw on operations after destroy - saveToS5', async () => {
      const session = await VectorDbSession.create(validConfig);
      await session.destroy();

      await assert.rejects(
        async () => await session.saveToS5(),
        (error) => {
          assert.ok(
            error.message.includes('destroyed') || error.message.includes('SESSION'),
            'Should mention session is destroyed'
          );
          return true;
        },
        'saveToS5 should throw after destroy'
      );
    });

    test('should throw on getStats after destroy', async () => {
      const session = await VectorDbSession.create(validConfig);
      await session.destroy();

      assert.throws(
        () => session.getStats(),
        (error) => {
          assert.ok(
            error.message.includes('destroyed') || error.message.includes('SESSION'),
            'Should mention session is destroyed'
          );
          return true;
        },
        'getStats should throw after destroy'
      );
    });
  });
});
