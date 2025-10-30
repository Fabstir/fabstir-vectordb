// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

const { describe, test, before, after } = require('node:test');
const assert = require('node:assert');
const { VectorDbSession } = require('../index.js');
const { startS5Service } = require('./helpers/s5-service.cjs');

// S5 service instance (started before all tests)
let s5Service = null;

// Test configuration
const validConfig = {
  s5Portal: 'http://127.0.0.1:5522',  // Use IP to avoid Docker hostname replacement
  userSeedPhrase: 'test-seed-phrase-for-deletion-tests-12345678901234567890',
  sessionId: 'test-session-deletion-tests',
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

// Start S5 service before all tests
before(async () => {
  console.log('Starting S5 service for deletion tests...');
  s5Service = await startS5Service({ port: 5522, mode: 'mock' });
});

// Stop S5 service after all tests
after(async () => {
  if (s5Service) {
    console.log('Stopping S5 service...');
    await s5Service.close();
  }
});

describe('VectorDBSession - deleteVector', () => {
  describe('Basic Deletion', () => {
    test('should delete single vector by ID', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        // Add test vectors
        const vectors = createTestVectors(10);
        await session.addVectors(vectors);

        // Verify vector exists before deletion
        const searchBefore = await session.search(vectors[5].vector, 10);
        const foundBefore = searchBefore.find(r => r.id === 'vec-5');
        assert.ok(foundBefore, 'Vector should exist before deletion');

        // Delete the vector
        await session.deleteVector('vec-5');

        // Verify vector no longer in search results
        const searchAfter = await session.search(vectors[5].vector, 10);
        const foundAfter = searchAfter.find(r => r.id === 'vec-5');
        assert.strictEqual(foundAfter, undefined, 'Deleted vector should not appear in search results');
      } finally {
        await session.destroy();
      }
    });

    test('should return success on delete', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        // Add test vectors
        const vectors = createTestVectors(5);
        await session.addVectors(vectors);

        // Delete should not throw
        await assert.doesNotReject(
          async () => await session.deleteVector('vec-2'),
          'Delete should succeed without throwing'
        );
      } finally {
        await session.destroy();
      }
    });

    test('should remove vector from metadata HashMap', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        // Add test vectors
        const vectors = createTestVectors(10);
        await session.addVectors(vectors);

        // Delete vector
        await session.deleteVector('vec-3');

        // Search using vec-4's vector (which should still exist)
        const query = vectors[4].vector;
        const results = await session.search(query, 10);

        // vec-3 should not be in results (deleted)
        const deletedVector = results.find(r => r.id === 'vec-3');
        assert.strictEqual(deletedVector, undefined, 'Deleted vector should not appear');

        // vec-4 should exist and have metadata
        const vec4 = results.find(r => r.id === 'vec-4');
        assert.ok(vec4, 'vec-4 should still exist');
        assert.ok(vec4.metadata, 'vec-4 should have metadata');
        assert.ok(vec4.metadata.title, 'Metadata should be intact');
      } finally {
        await session.destroy();
      }
    });
  });

  describe('Error Handling', () => {
    test('should throw error when deleting non-existent vector', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        // Add test vectors
        const vectors = createTestVectors(5);
        await session.addVectors(vectors);

        // Try to delete non-existent vector
        await assert.rejects(
          async () => await session.deleteVector('vec-nonexistent'),
          (error) => {
            assert.ok(
              error.message.includes('not found') || error.message.includes('does not exist'),
              `Error message should indicate vector not found, got: ${error.message}`
            );
            return true;
          },
          'Should throw error for non-existent vector'
        );
      } finally {
        await session.destroy();
      }
    });

    test('should throw error when deleting from empty index', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        // Don't add any vectors - try to delete from empty index
        await assert.rejects(
          async () => await session.deleteVector('vec-0'),
          (error) => {
            assert.ok(
              error.message.includes('not found') || error.message.includes('does not exist') || error.message.includes('not initialized'),
              `Error message should indicate vector not found or index not initialized, got: ${error.message}`
            );
            return true;
          },
          'Should throw error when deleting from empty index'
        );
      } finally {
        await session.destroy();
      }
    });
  });

  describe('Multiple Deletions', () => {
    test('should handle multiple deletes sequentially', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        // Add test vectors
        const vectors = createTestVectors(10);
        await session.addVectors(vectors);

        // Delete multiple vectors
        await session.deleteVector('vec-0');
        await session.deleteVector('vec-1');
        await session.deleteVector('vec-2');

        // Search using vec-3's vector (which should still exist)
        const query = vectors[3].vector;
        const results = await session.search(query, 10);

        assert.strictEqual(
          results.find(r => r.id === 'vec-0'),
          undefined,
          'vec-0 should not appear'
        );
        assert.strictEqual(
          results.find(r => r.id === 'vec-1'),
          undefined,
          'vec-1 should not appear'
        );
        assert.strictEqual(
          results.find(r => r.id === 'vec-2'),
          undefined,
          'vec-2 should not appear'
        );

        // vec-3 should still exist (we're searching with its vector)
        assert.ok(
          results.find(r => r.id === 'vec-3'),
          'Non-deleted vectors should still exist'
        );
      } finally {
        await session.destroy();
      }
    });

    test('should handle deleting same vector twice (idempotent)', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        // Add test vectors
        const vectors = createTestVectors(5);
        await session.addVectors(vectors);

        // First deletion should succeed
        await session.deleteVector('vec-2');

        // Second deletion of same vector
        // Should either succeed (idempotent) or throw appropriate error
        try {
          await session.deleteVector('vec-2');
          // If it succeeds, that's fine (idempotent behavior)
        } catch (error) {
          // If it fails, error should indicate vector not found
          assert.ok(
            error.message.includes('not found') || error.message.includes('does not exist'),
            'Error should indicate vector not found'
          );
        }
      } finally {
        await session.destroy();
      }
    });
  });

  describe('Delete and Re-add', () => {
    test('should prevent re-adding vector with same ID after soft deletion', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        // Add multiple initial vectors (at least 10 for IVF training)
        const vectors = createTestVectors(10);
        await session.addVectors(vectors);

        // Delete one vector
        await session.deleteVector('vec-5');

        // Verify deleted
        const searchAfterDelete = await session.search(vectors[5].vector, 5);
        const foundDeleted = searchAfterDelete.find(r => r.id === 'vec-5');
        assert.strictEqual(foundDeleted, undefined, 'Deleted vector should not appear in search');

        // Try to re-add vector with same ID
        // This should fail because soft deletion doesn't physically remove the vector
        const newVector = [{
          id: 'vec-5',
          vector: vectors[5].vector,
          metadata: {
            title: 'Re-added Vector',
            views: 999,
            tags: ['new', 'replaced'],
          }
        }];

        // Should throw error because vector still exists (soft deleted)
        await assert.rejects(
          async () => await session.addVectors(newVector),
          (error) => {
            assert.ok(
              error.message.includes('already exists'),
              `Error should indicate vector already exists, got: ${error.message}`
            );
            return true;
          },
          'Should not allow re-adding soft-deleted vector'
        );
      } finally {
        await session.destroy();
      }
    });
  });

  describe('Stats and Persistence', () => {
    test('should reduce vector count in getStats after deletion', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        // Add test vectors
        const vectors = createTestVectors(10);
        await session.addVectors(vectors);

        // Get stats before deletion
        const statsBefore = await session.getStats();
        assert.strictEqual(statsBefore.vectorCount, 10, 'Should have 10 vectors initially');

        // Delete 3 vectors
        await session.deleteVector('vec-0');
        await session.deleteVector('vec-1');
        await session.deleteVector('vec-2');

        // Get stats after deletion
        const statsAfter = await session.getStats();
        assert.strictEqual(statsAfter.vectorCount, 7, 'Should have 7 vectors after deletion (10 - 3)');
      } finally {
        await session.destroy();
      }
    });
  });
});
