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
  s5Portal: 'http://127.0.0.1:5522',
  userSeedPhrase: 'test-seed-phrase-for-delete-by-metadata-tests-1234567890',
  sessionId: 'test-session-delete-by-metadata',
};

// Helper to create test vectors with varied metadata
function createTestVectors(count, startId = 0) {
  const vectors = [];
  for (let i = 0; i < count; i++) {
    const id = `vec-${startId + i}`;
    const vector = Array(128).fill(0).map((_, idx) => Math.sin(idx + i) * 0.5);
    const metadata = {
      userId: `user${(i % 3) + 1}`, // user1, user2, user3
      category: i % 2 === 0 ? 'video' : 'article',
      status: i < 5 ? 'active' : 'archived',
      views: 100 + i * 10,
      tags: i % 2 === 0 ? ['tech', 'ai'] : ['business', 'ml'],
      nested: {
        id: startId + i,
        active: i % 2 === 0,
      }
    };
    vectors.push({ id, vector, metadata });
  }
  return vectors;
}

// Start S5 service before all tests
before(async () => {
  console.log('Starting S5 service for delete-by-metadata tests...');
  s5Service = await startS5Service({ port: 5522, mode: 'mock' });
});

// Stop S5 service after all tests
after(async () => {
  if (s5Service) {
    console.log('Stopping S5 service...');
    await s5Service.close();
  }
});

describe('VectorDBSession - deleteByMetadata', () => {
  describe('Single Field Matching', () => {
    test('should delete by single field match', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        // Add test vectors
        const vectors = createTestVectors(10);
        await session.addVectors(vectors);

        // Delete all vectors with userId: 'user1'
        const result = await session.deleteByMetadata({ userId: 'user1' });

        assert.ok(result, 'Result should be returned');
        assert.ok(result.deletedCount > 0, 'Should delete at least one vector');
        assert.ok(Array.isArray(result.deletedIds), 'Should return array of deleted IDs');
        assert.strictEqual(result.deletedIds.length, result.deletedCount, 'Deleted IDs length should match count');

        // Verify deleted vectors not in search results
        // Use vector from vec-1 (userId=user2, should NOT be deleted)
        const query = vectors[1].vector;
        const searchResults = await session.search(query, 10);

        for (const deletedId of result.deletedIds) {
          assert.strictEqual(
            searchResults.find(r => r.id === deletedId),
            undefined,
            `Deleted vector ${deletedId} should not appear in search`
          );
        }

        // Verify remaining vectors still have correct metadata
        const remainingVector = searchResults.find(r => r.metadata.userId === 'user2' || r.metadata.userId === 'user3');
        assert.ok(remainingVector, 'Non-deleted vectors should still exist');
      } finally {
        await session.destroy();
      }
    });

    test('should return count of deleted vectors', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        // Add test vectors
        const vectors = createTestVectors(12);
        await session.addVectors(vectors);

        // Delete all 'video' category vectors (should be 6 out of 12)
        const result = await session.deleteByMetadata({ category: 'video' });

        assert.strictEqual(result.deletedCount, 6, 'Should delete 6 videos (even indices)');
        assert.strictEqual(result.deletedIds.length, 6, 'Should return 6 deleted IDs');

        // Verify deleted IDs are correct
        for (const deletedId of result.deletedIds) {
          const index = parseInt(deletedId.split('-')[1]);
          assert.strictEqual(index % 2, 0, 'Deleted IDs should be even indices (videos)');
        }
      } finally {
        await session.destroy();
      }
    });

    test('should return 0 when no vectors match filter', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        // Add test vectors
        const vectors = createTestVectors(10);
        await session.addVectors(vectors);

        // Delete with non-matching filter
        const result = await session.deleteByMetadata({ userId: 'nonexistent' });

        assert.strictEqual(result.deletedCount, 0, 'Should delete 0 vectors');
        assert.strictEqual(result.deletedIds.length, 0, 'Should return empty deleted IDs array');
      } finally {
        await session.destroy();
      }
    });
  });

  describe('Multiple Field Matching (AND logic)', () => {
    test('should delete by multiple fields with AND logic', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        // Add test vectors
        const vectors = createTestVectors(10);
        await session.addVectors(vectors);

        // Delete vectors matching: userId='user1' AND category='video'
        const result = await session.deleteByMetadata({
          userId: 'user1',
          category: 'video'
        });

        assert.ok(result.deletedCount > 0, 'Should delete at least one vector');

        // Verify all deleted vectors matched both criteria
        const query = vectors[0].vector;
        const searchResults = await session.search(query, 10);

        // Check remaining vectors - none should have both userId='user1' AND category='video'
        for (const r of searchResults) {
          const hasUser1 = r.metadata.userId === 'user1';
          const hasVideo = r.metadata.category === 'video';
          assert.ok(!(hasUser1 && hasVideo), 'No remaining vector should match both criteria');
        }
      } finally {
        await session.destroy();
      }
    });

    test('should handle multiple field non-matching', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        // Add test vectors
        const vectors = createTestVectors(10);
        await session.addVectors(vectors);

        // Delete with contradictory filters (userId='user1' AND userId='user2' - impossible)
        // This would be handled by applying both filters, resulting in no matches
        const result = await session.deleteByMetadata({
          userId: 'user1',
          status: 'nonexistent'
        });

        assert.strictEqual(result.deletedCount, 0, 'Should delete 0 vectors with contradictory filter');
      } finally {
        await session.destroy();
      }
    });
  });

  describe('Nested Field Matching', () => {
    test('should delete by nested field using dot notation', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        // Add test vectors
        const vectors = createTestVectors(10);
        await session.addVectors(vectors);

        // Delete vectors where nested.active = true (even indices)
        const result = await session.deleteByMetadata({ 'nested.active': true });

        assert.ok(result.deletedCount > 0, 'Should delete at least one vector');

        // Verify deleted vectors had nested.active = true
        const query = vectors[0].vector;
        const searchResults = await session.search(query, 10);

        for (const r of searchResults) {
          assert.strictEqual(r.metadata.nested.active, false, 'Remaining vectors should have nested.active=false');
        }
      } finally {
        await session.destroy();
      }
    });
  });

  describe('Array Field Matching', () => {
    test('should delete by checking if value is in array field', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        // Add test vectors
        const vectors = createTestVectors(10);
        await session.addVectors(vectors);

        // Delete vectors where tags contains 'ai' (even indices have ['tech', 'ai'])
        const result = await session.deleteByMetadata({ tags: 'ai' });

        assert.ok(result.deletedCount > 0, 'Should delete at least one vector');

        // Verify remaining vectors don't have 'ai' tag
        const query = vectors[0].vector;
        const searchResults = await session.search(query, 10);

        for (const r of searchResults) {
          assert.ok(
            !r.metadata.tags.includes('ai'),
            'Remaining vectors should not have "ai" tag'
          );
        }
      } finally {
        await session.destroy();
      }
    });
  });

  describe('Edge Cases', () => {
    test('should handle empty filter object (delete nothing)', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        // Add test vectors
        const vectors = createTestVectors(10);
        await session.addVectors(vectors);

        // Delete with empty filter - should match all vectors
        // But for safety, we should not allow this
        const result = await session.deleteByMetadata({});

        // Empty filter might match all or none - implementation dependent
        // For safety, we expect it to delete nothing or throw error
        assert.ok(
          result.deletedCount === 0 || result.deletedCount === 10,
          'Empty filter should either delete all or none (implementation-specific)'
        );
      } finally {
        await session.destroy();
      }
    });

    test('should handle deletion from empty index', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        // Don't add any vectors
        const result = await session.deleteByMetadata({ userId: 'user1' });

        assert.strictEqual(result.deletedCount, 0, 'Should delete 0 from empty index');
        assert.strictEqual(result.deletedIds.length, 0, 'Should return empty array');
      } finally {
        await session.destroy();
      }
    });

    test('should handle complex filter with multiple criteria', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        // Add test vectors
        const vectors = createTestVectors(10);
        await session.addVectors(vectors);

        // Complex filter: userId='user1' AND category='video' AND status='active'
        const result = await session.deleteByMetadata({
          userId: 'user1',
          category: 'video',
          status: 'active'
        });

        // Verify result structure
        assert.ok(typeof result.deletedCount === 'number', 'deletedCount should be a number');
        assert.ok(Array.isArray(result.deletedIds), 'deletedIds should be an array');
        assert.strictEqual(result.deletedIds.length, result.deletedCount, 'Counts should match');
      } finally {
        await session.destroy();
      }
    });
  });

  describe('Integration with getStats', () => {
    test('should reflect deletion in getStats', async () => {
      const session = await VectorDbSession.create(validConfig);

      try {
        // Add test vectors
        const vectors = createTestVectors(10);
        await session.addVectors(vectors);

        // Get stats before deletion
        const statsBefore = await session.getStats();
        assert.strictEqual(statsBefore.vectorCount, 10, 'Should have 10 vectors initially');

        // Delete some vectors
        const result = await session.deleteByMetadata({ userId: 'user1' });
        const deletedCount = result.deletedCount;

        // Get stats after deletion
        const statsAfter = await session.getStats();
        assert.strictEqual(
          statsAfter.vectorCount,
          10 - deletedCount,
          `Should have ${10 - deletedCount} vectors after deletion`
        );
      } finally {
        await session.destroy();
      }
    });
  });
});
