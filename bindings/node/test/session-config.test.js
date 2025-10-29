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
  console.log('Starting S5 service for config tests...');
  s5Service = await startS5Service({ port: 5522, mode: 'mock' });
});

// Stop S5 service after all tests
after(async () => {
  if (s5Service) {
    console.log('Stopping S5 service...');
    await s5Service.close();
  }
});

describe('Session Configuration', () => {
  describe('Encryption Configuration', () => {
    test('should default to encryption ON when not specified', async () => {
      const config = {
        s5Portal: 'http://127.0.0.1:5522',
        userSeedPhrase: 'test-config-default-encryption-seed',
        sessionId: 'test-config-default-encryption',
      };

      const session = await VectorDbSession.create(config);

      try {
        assert.ok(session, 'Session should be created');

        // Session should work with default encryption (need at least 3 vectors for IVF)
        const vectors = [
          { id: 'test-1', vector: Array(128).fill(0).map((_, i) => Math.sin(i) * 0.5), metadata: { test: 'encryption-default' }},
          { id: 'test-2', vector: Array(128).fill(0).map((_, i) => Math.cos(i) * 0.5), metadata: { test: 'encryption-default' }},
          { id: 'test-3', vector: Array(128).fill(0).map((_, i) => Math.tan(i) * 0.5), metadata: { test: 'encryption-default' }}
        ];

        await session.addVectors(vectors);
        const results = await session.search(vectors[0].vector, 1);

        assert.strictEqual(results.length, 1, 'Should find the vector');
        assert.ok(results[0].score > 0.99, 'Should have high similarity score for exact match');
      } finally {
        await session.destroy();
      }
    });

    test('should accept explicit encryption enabled', async () => {
      const config = {
        s5Portal: 'http://127.0.0.1:5522',
        userSeedPhrase: 'test-config-encryption-on-seed',
        sessionId: 'test-config-encryption-on',
        encryptAtRest: true,  // Explicitly enable
      };

      const session = await VectorDbSession.create(config);

      try {
        assert.ok(session, 'Session should be created');

        // Need at least 3 vectors for IVF training
        const vectors = [
          { id: 'test-encrypted-1', vector: Array(128).fill(0).map((_, i) => Math.cos(i) * 0.5), metadata: { encrypted: true }},
          { id: 'test-encrypted-2', vector: Array(128).fill(0).map((_, i) => Math.sin(i) * 0.5), metadata: { encrypted: true }},
          { id: 'test-encrypted-3', vector: Array(128).fill(0).map((_, i) => Math.tan(i) * 0.5), metadata: { encrypted: true }}
        ];

        await session.addVectors(vectors);
        const results = await session.search(vectors[0].vector, 1);

        assert.strictEqual(results.length, 1, 'Should find encrypted vector');
      } finally {
        await session.destroy();
      }
    });

    test('should allow disabling encryption', async () => {
      const config = {
        s5Portal: 'http://127.0.0.1:5522',
        userSeedPhrase: 'test-config-encryption-off-seed',
        sessionId: 'test-config-encryption-off',
        encryptAtRest: false,  // Explicitly disable
      };

      const session = await VectorDbSession.create(config);

      try {
        assert.ok(session, 'Session should be created with encryption disabled');

        // Need at least 3 vectors for IVF training
        const vectors = [
          { id: 'test-unencrypted-1', vector: Array(128).fill(0).map((_, i) => Math.sin(i + 1) * 0.5), metadata: { encrypted: false }},
          { id: 'test-unencrypted-2', vector: Array(128).fill(0).map((_, i) => Math.cos(i + 1) * 0.5), metadata: { encrypted: false }},
          { id: 'test-unencrypted-3', vector: Array(128).fill(0).map((_, i) => Math.tan(i + 1) * 0.5), metadata: { encrypted: false }}
        ];

        await session.addVectors(vectors);
        const results = await session.search(vectors[0].vector, 1);

        assert.strictEqual(results.length, 1, 'Should find unencrypted vector');
      } finally {
        await session.destroy();
      }
    });
  });

  describe('Chunking Configuration', () => {
    test('should accept custom chunk_size', async () => {
      const config = {
        s5Portal: 'http://127.0.0.1:5522',
        userSeedPhrase: 'test-config-chunk-size-seed',
        sessionId: 'test-config-chunk-size',
        chunkSize: 5000,  // Custom chunk size (default is 10000)
      };

      const session = await VectorDbSession.create(config);

      try {
        assert.ok(session, 'Session should be created with custom chunk size');

        // Add some vectors
        const vectors = [];
        for (let i = 0; i < 10; i++) {
          vectors.push({
            id: `chunk-test-${i}`,
            vector: Array(128).fill(0).map((_, idx) => Math.sin(idx + i) * 0.5),
            metadata: { chunkTest: i }
          });
        }

        await session.addVectors(vectors);
        const results = await session.search(vectors[0].vector, 5);

        assert.ok(results.length > 0, 'Should find vectors with custom chunk size');
      } finally {
        await session.destroy();
      }
    });

    test('should accept custom cache_size_mb', async () => {
      const config = {
        s5Portal: 'http://127.0.0.1:5522',
        userSeedPhrase: 'test-config-cache-size-seed',
        sessionId: 'test-config-cache-size',
        cacheSizeMb: 100,  // Custom cache size (default is 150)
      };

      const session = await VectorDbSession.create(config);

      try {
        assert.ok(session, 'Session should be created with custom cache size');

        // Need at least 3 vectors for IVF training
        const vectors = [
          { id: 'cache-test-1', vector: Array(128).fill(0).map((_, i) => Math.tan(i) * 0.5), metadata: { cacheTest: true }},
          { id: 'cache-test-2', vector: Array(128).fill(0).map((_, i) => Math.sin(i) * 0.5), metadata: { cacheTest: true }},
          { id: 'cache-test-3', vector: Array(128).fill(0).map((_, i) => Math.cos(i) * 0.5), metadata: { cacheTest: true }}
        ];

        await session.addVectors(vectors);
        const results = await session.search(vectors[0].vector, 1);

        assert.strictEqual(results.length, 1, 'Should work with custom cache size');
      } finally {
        await session.destroy();
      }
    });

    test('should accept all custom configs together', async () => {
      const config = {
        s5Portal: 'http://127.0.0.1:5522',
        userSeedPhrase: 'test-config-all-custom-seed',
        sessionId: 'test-config-all-custom',
        encryptAtRest: true,
        chunkSize: 8000,
        cacheSizeMb: 200,
        memoryBudgetMb: 1024,
      };

      const session = await VectorDbSession.create(config);

      try {
        assert.ok(session, 'Session should be created with all custom configs');

        const vectors = [];
        for (let i = 0; i < 20; i++) {
          vectors.push({
            id: `all-config-${i}`,
            vector: Array(128).fill(0).map((_, idx) => Math.sin(idx * i) * 0.5),
            metadata: { index: i }
          });
        }

        await session.addVectors(vectors);
        const results = await session.search(vectors[0].vector, 10);

        assert.ok(results.length > 0, 'Should work with all custom configs');
      } finally {
        await session.destroy();
      }
    });
  });

  describe('Configuration Validation', () => {
    test('should reject invalid chunk_size (zero)', async () => {
      const config = {
        s5Portal: 'http://127.0.0.1:5522',
        userSeedPhrase: 'test-config-invalid-chunk-seed',
        sessionId: 'test-config-invalid-chunk',
        chunkSize: 0,  // Invalid
      };

      await assert.rejects(
        async () => await VectorDbSession.create(config),
        (err) => {
          assert.ok(err.message.includes('chunk') || err.message.includes('invalid') || err.message.includes('zero'),
            'Error should mention invalid chunk size');
          return true;
        },
        'Should reject zero chunk_size'
      );
    });

    test('should reject invalid cache_size_mb (zero)', async () => {
      const config = {
        s5Portal: 'http://127.0.0.1:5522',
        userSeedPhrase: 'test-config-invalid-cache-seed',
        sessionId: 'test-config-invalid-cache',
        cacheSizeMb: 0,  // Invalid
      };

      await assert.rejects(
        async () => await VectorDbSession.create(config),
        (err) => {
          assert.ok(err.message.includes('cache') || err.message.includes('invalid') || err.message.includes('zero'),
            'Error should mention invalid cache size');
          return true;
        },
        'Should reject zero cache_size_mb'
      );
    });
  });
});
