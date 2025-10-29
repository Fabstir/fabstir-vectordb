// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

const { describe, test, before, after } = require('node:test');
const assert = require('node:assert');
const { VectorDbSession } = require('../index.js');
const { startS5Service } = require('./helpers/s5-service.cjs');

// S5 service instance (started before all tests)
let s5Service = null;

// Start S5 service before all tests
before(async () => {
  console.log('Starting S5 service for chunked load tests...');
  s5Service = await startS5Service({ port: 5523, mode: 'mock' });
});

// Stop S5 service after all tests
after(async () => {
  if (s5Service) {
    console.log('Stopping S5 service...');
    await s5Service.close();
  }
});

describe('Chunked Loading', () => {
  describe('Basic Chunked Load', () => {
    test('should load index saved with chunked format (encryption ON)', async () => {
      const config = {
        s5Portal: 'http://127.0.0.1:5523',
        userSeedPhrase: 'test-chunked-load-enc-on-seed',
        sessionId: 'test-chunked-load-enc-on',
        encryptAtRest: true,
        chunkSize: 1000, // Small chunks for faster tests
      };

      const session1 = await VectorDbSession.create(config);

      try {
        // Create and save vectors
        const vectors = [];
        for (let i = 0; i < 1500; i++) {
          vectors.push({
            id: `doc-${i}`,
            vector: Array(128).fill(0).map((_, idx) => Math.sin(idx + i) * 0.5),
            metadata: { docId: i, encrypted: true }
          });
        }

        await session1.addVectors(vectors);
        const cid = await session1.saveToS5();
        await session1.destroy();

        // Load in new session
        const session2 = await VectorDbSession.create(config);
        await session2.loadUserVectors(cid);

        // Verify vectors loaded correctly
        const stats = session2.getStats();
        assert.strictEqual(stats.vectorCount, 1500, 'Should load all 1500 vectors');
        assert.ok(stats.vectorCount > 0, 'Vector count should be positive');

        await session2.destroy();
      } finally {
        // Cleanup
        if (session1) await session1.destroy().catch(() => {});
      }
    });

    test('should load index saved with chunked format (encryption OFF)', async () => {
      const config = {
        s5Portal: 'http://127.0.0.1:5523',
        userSeedPhrase: 'test-chunked-load-enc-off-seed',
        sessionId: 'test-chunked-load-enc-off',
        encryptAtRest: false,
        chunkSize: 1000,
      };

      const session1 = await VectorDbSession.create(config);

      try {
        // Create and save vectors
        const vectors = [];
        for (let i = 0; i < 1500; i++) {
          vectors.push({
            id: `doc-${i}`,
            vector: Array(128).fill(0).map((_, idx) => Math.cos(idx + i) * 0.5),
            metadata: { docId: i, encrypted: false }
          });
        }

        await session1.addVectors(vectors);
        const cid = await session1.saveToS5();
        await session1.destroy();

        // Load in new session
        const session2 = await VectorDbSession.create(config);
        await session2.loadUserVectors(cid);

        // Verify vectors loaded correctly
        const stats = session2.getStats();
        assert.strictEqual(stats.vectorCount, 1500, 'Should load all 1500 vectors');

        await session2.destroy();
      } finally {
        if (session1) await session1.destroy().catch(() => {});
      }
    });
  });

  describe('Search After Chunked Load', () => {
    test('should search correctly after loading chunked index', async () => {
      const config = {
        s5Portal: 'http://127.0.0.1:5523',
        userSeedPhrase: 'test-chunked-search-seed',
        sessionId: 'test-chunked-search',
        encryptAtRest: true,
        chunkSize: 500,
      };

      const session1 = await VectorDbSession.create(config);

      try {
        // Create test vectors with known patterns
        const vectors = [];
        for (let i = 0; i < 1000; i++) {
          vectors.push({
            id: `search-doc-${i}`,
            vector: Array(128).fill(0).map((_, idx) => Math.sin(idx + i * 0.1) * 0.5),
            metadata: { index: i }
          });
        }

        await session1.addVectors(vectors);

        // Search before save
        const queryVector = vectors[5].vector;
        const resultsBefore = await session1.search(queryVector, 3);
        assert.ok(resultsBefore.length > 0, 'Should find results before save');

        const cid = await session1.saveToS5();
        await session1.destroy();

        // Load in new session
        const session2 = await VectorDbSession.create(config);
        await session2.loadUserVectors(cid);

        // Search after load
        const resultsAfter = await session2.search(queryVector, 3);
        assert.ok(resultsAfter.length > 0, 'Should find results after load');
        assert.ok(resultsAfter[0].score > 0.95, 'Top result should have high similarity');

        await session2.destroy();
      } finally {
        if (session1) await session1.destroy().catch(() => {});
      }
    });
  });

  describe('Add Vectors After Chunked Load', () => {
    test('should add new vectors after loading chunked index', async () => {
      const config = {
        s5Portal: 'http://127.0.0.1:5523',
        userSeedPhrase: 'test-chunked-add-seed',
        sessionId: 'test-chunked-add',
        encryptAtRest: true,
        chunkSize: 500,
      };

      const session1 = await VectorDbSession.create(config);

      try {
        // Create initial vectors
        const vectors1 = [];
        for (let i = 0; i < 800; i++) {
          vectors1.push({
            id: `initial-${i}`,
            vector: Array(128).fill(0).map((_, idx) => Math.sin(idx + i) * 0.5),
            metadata: { batch: 'initial', index: i }
          });
        }

        await session1.addVectors(vectors1);
        const cid = await session1.saveToS5();
        await session1.destroy();

        // Load in new session
        const session2 = await VectorDbSession.create(config);
        await session2.loadUserVectors(cid);

        // Verify initial count
        let stats = session2.getStats();
        assert.strictEqual(stats.vectorCount, 800, 'Should have 800 initial vectors');

        // Add more vectors
        const vectors2 = [];
        for (let i = 0; i < 300; i++) {
          vectors2.push({
            id: `additional-${i}`,
            vector: Array(128).fill(0).map((_, idx) => Math.cos(idx + i) * 0.5),
            metadata: { batch: 'additional', index: i }
          });
        }

        await session2.addVectors(vectors2);

        // Verify updated count
        stats = session2.getStats();
        assert.strictEqual(stats.vectorCount, 1100, 'Should have 1100 total vectors after adding');

        await session2.destroy();
      } finally {
        if (session1) await session1.destroy().catch(() => {});
      }
    });
  });

  describe('Save-Load Roundtrip', () => {
    test('should preserve all data through save-load-save cycle', async () => {
      const config = {
        s5Portal: 'http://127.0.0.1:5523',
        userSeedPhrase: 'test-chunked-roundtrip-seed',
        sessionId: 'test-chunked-roundtrip',
        encryptAtRest: true,
        chunkSize: 500,
      };

      const session1 = await VectorDbSession.create(config);

      try {
        // Create test vectors
        const vectors = [];
        for (let i = 0; i < 1000; i++) {
          vectors.push({
            id: `roundtrip-${i}`,
            vector: Array(128).fill(0).map((_, idx) => Math.sin(idx + i * 0.1) * 0.5),
            metadata: { roundtrip: true, index: i }
          });
        }

        await session1.addVectors(vectors);
        const cid1 = await session1.saveToS5();
        const stats1 = session1.getStats();
        await session1.destroy();

        // First load
        const session2 = await VectorDbSession.create(config);
        await session2.loadUserVectors(cid1);
        const stats2 = session2.getStats();

        // Save again
        const cid2 = await session2.saveToS5();
        await session2.destroy();

        // Second load
        const session3 = await VectorDbSession.create(config);
        await session3.loadUserVectors(cid2);
        const stats3 = session3.getStats();

        // Verify consistency
        assert.strictEqual(stats2.vectorCount, stats1.vectorCount, 'First load should match original');
        assert.strictEqual(stats3.vectorCount, stats1.vectorCount, 'Second load should match original');

        await session3.destroy();
      } finally {
        if (session1) await session1.destroy().catch(() => {});
      }
    });
  });

  describe('Large Dataset Loading', () => {
    test('should load large dataset efficiently (5K vectors, 5 chunks)', async () => {
      const config = {
        s5Portal: 'http://127.0.0.1:5523',
        userSeedPhrase: 'test-chunked-large-seed',
        sessionId: 'test-chunked-large',
        encryptAtRest: true,
        chunkSize: 1000, // 5K vectors = 5 chunks
        cacheSizeMb: 100,
      };

      const session1 = await VectorDbSession.create(config);

      try {
        // Create large dataset
        const vectors = [];
        for (let i = 0; i < 5000; i++) {
          vectors.push({
            id: `large-doc-${i}`,
            vector: Array(128).fill(0).map((_, idx) => Math.sin(idx + i * 0.01) * 0.5),
            metadata: { docId: i, category: `cat-${i % 10}` }
          });
        }

        console.log('Adding 5000 vectors...');
        await session1.addVectors(vectors);

        console.log('Saving to S5...');
        const start = Date.now();
        const cid = await session1.saveToS5();
        const saveTime = Date.now() - start;
        console.log(`Save completed in ${saveTime}ms`);

        await session1.destroy();

        // Load large dataset
        const session2 = await VectorDbSession.create(config);

        console.log('Loading from S5...');
        const loadStart = Date.now();
        await session2.loadUserVectors(cid);
        const loadTime = Date.now() - loadStart;
        console.log(`Load completed in ${loadTime}ms`);

        // Verify
        const stats = session2.getStats();
        assert.strictEqual(stats.vectorCount, 5000, 'Should load all 5000 vectors');

        // Test search performance
        const searchStart = Date.now();
        const searchResults = await session2.search(vectors[0].vector, 10);
        const searchTime = Date.now() - searchStart;
        console.log(`Search completed in ${searchTime}ms`);

        assert.ok(searchResults.length > 0, 'Should return search results');
        assert.ok(searchTime < 500, 'Search should complete in under 500ms');

        await session2.destroy();
      } finally {
        if (session1) await session1.destroy().catch(() => {});
      }
    });
  });

  describe('Error Handling', () => {
    test('should handle missing manifest gracefully', async () => {
      const config = {
        s5Portal: 'http://127.0.0.1:5523',
        userSeedPhrase: 'test-chunked-missing-manifest-seed',
        sessionId: 'test-chunked-missing-manifest',
      };

      const session = await VectorDbSession.create(config);

      try {
        // Try to load from non-existent path
        await assert.rejects(
          async () => await session.loadUserVectors('nonexistent-cid-12345'),
          (err) => {
            assert.ok(
              err.message.includes('manifest') || err.message.includes('Missing') || err.message.includes('not found'),
              'Error should mention missing manifest or component'
            );
            return true;
          },
          'Should reject with missing manifest error'
        );
      } finally {
        await session.destroy();
      }
    });

    test('should handle empty index gracefully', async () => {
      const config = {
        s5Portal: 'http://127.0.0.1:5523',
        userSeedPhrase: 'test-chunked-empty-seed',
        sessionId: 'test-chunked-empty',
        encryptAtRest: true,
      };

      const session1 = await VectorDbSession.create(config);

      try {
        // Save empty index
        const cid = await session1.saveToS5();
        await session1.destroy();

        // Load empty index
        const session2 = await VectorDbSession.create(config);
        await session2.loadUserVectors(cid);

        const stats = session2.getStats();
        assert.strictEqual(stats.vectorCount, 0, 'Empty index should have 0 vectors');

        await session2.destroy();
      } finally {
        if (session1) await session1.destroy().catch(() => {});
      }
    });
  });
});
