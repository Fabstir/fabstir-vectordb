// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

/**
 * Integration tests for updateMetadata functionality
 * Tests metadata updates for existing vectors with proper error handling
 */

const { test, before, after } = require('node:test');
const assert = require('node:assert');
const { VectorDbSession } = require('../index.js');
const { startS5Service } = require('./helpers/s5-service.cjs');

// S5 service instance (started before all tests)
let s5Service = null;

// Test configuration
const TEST_CONFIG = {
  s5Portal: 'http://localhost:5522',
  userSeedPhrase: 'test-seed-phrase-update-metadata',
  sessionId: 'test-session-update-metadata',
  encryptAtRest: true,
  chunkSize: 10000,
  cacheSizeMb: 150,
};

// Helper to generate test vector (384 dimensions for all-MiniLM-L6-v2)
function generateTestVector(seed = 0) {
  const vector = [];
  for (let i = 0; i < 384; i++) {
    vector.push(Math.sin(seed + i) * 0.5);
  }
  return vector;
}

// Helper to check if two vectors are approximately equal
function vectorsApproxEqual(v1, v2, tolerance = 1e-6) {
  if (v1.length !== v2.length) return false;
  for (let i = 0; i < v1.length; i++) {
    if (Math.abs(v1[i] - v2[i]) > tolerance) return false;
  }
  return true;
}

// Helper to add training vectors (IVF requires at least 10 vectors)
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
  console.log('Starting S5 service for update-metadata tests...');
  s5Service = await startS5Service({ port: 5522, mode: 'mock' });
});

// Stop S5 service after all tests
after(async () => {
  if (s5Service) {
    console.log('Stopping S5 service...');
    await s5Service.close();
  }
});

test('updateMetadata - update metadata for existing vector', async (t) => {
  const session = await VectorDbSession.create(TEST_CONFIG);

  try {
    // Add training vectors (IVF requires at least 10 vectors)
    await addTrainingVectors(session);

    // Add a vector with initial metadata
    const vector = generateTestVector(1);
    await session.addVectors([{
      id: 'doc1',
      vector,
      metadata: {
        text: 'Initial text',
        timestamp: 1000,
        category: 'initial',
      },
    }]);

    // Update metadata
    const newMetadata = {
      text: 'Updated text',
      timestamp: 2000,
      category: 'updated',
      newField: 'additional data',
    };

    await session.updateMetadata('doc1', newMetadata);

    // Retrieve and verify (search with k=20 to ensure we find it among training vectors)
    const results = await session.search(vector, 20);
    assert.ok(results.length > 0, 'Should find vectors');

    const doc1Result = results.find(r => r.id === 'doc1');
    assert.ok(doc1Result, 'Should find doc1');
    assert.deepStrictEqual(doc1Result.metadata, newMetadata, 'Metadata should be updated');
  } finally {
    await session.destroy();
  }
});

test('updateMetadata - updated metadata returned in search results', async (t) => {
  const session = await VectorDbSession.create(TEST_CONFIG);

  try {
    // Add multiple vectors
    await session.addVectors([
      {
        id: 'vec1',
        vector: generateTestVector(1),
        metadata: { title: 'First', score: 10 },
      },
      {
        id: 'vec2',
        vector: generateTestVector(2),
        metadata: { title: 'Second', score: 20 },
      },
      {
        id: 'vec3',
        vector: generateTestVector(3),
        metadata: { title: 'Third', score: 30 },
      },
    ]);

    // Update metadata for vec2
    await session.updateMetadata('vec2', {
      title: 'Second Updated',
      score: 99,
      extra: 'new field',
    });

    // Search and verify updated metadata appears
    const queryVector = generateTestVector(2);
    const results = await session.search(queryVector, 20);

    const vec2Result = results.find(r => r.id === 'vec2');
    assert.ok(vec2Result, 'Should find vec2 in search results');
    assert.strictEqual(vec2Result.metadata.title, 'Second Updated', 'Title should be updated');
    assert.strictEqual(vec2Result.metadata.score, 99, 'Score should be updated');
    assert.strictEqual(vec2Result.metadata.extra, 'new field', 'New field should exist');
  } finally {
    await session.destroy();
  }
});

test('updateMetadata - update replaces entire metadata object', async (t) => {
  const session = await VectorDbSession.create(TEST_CONFIG);

  try {
    // Add training vectors
    await addTrainingVectors(session);

    // Add vector with complex metadata
    const vector = generateTestVector(5);
    await session.addVectors([{
      id: 'complex',
      vector,
      metadata: {
        field1: 'value1',
        field2: 'value2',
        field3: 'value3',
        nested: { a: 1, b: 2 },
      },
    }]);

    // Update with completely different metadata (replace, not merge)
    const newMetadata = {
      differentField: 'new value',
      anotherField: 42,
    };

    await session.updateMetadata('complex', newMetadata);

    // Verify old fields are gone
    const results = await session.search(vector, 1);
    assert.strictEqual(results.length, 1);
    assert.deepStrictEqual(results[0].metadata, newMetadata, 'Should completely replace metadata');
    assert.strictEqual(results[0].metadata.field1, undefined, 'Old field1 should not exist');
    assert.strictEqual(results[0].metadata.nested, undefined, 'Old nested should not exist');
  } finally {
    await session.destroy();
  }
});

test('updateMetadata - update non-existent vector throws error', async (t) => {
  const session = await VectorDbSession.create(TEST_CONFIG);

  try {
    // Try to update metadata for non-existent vector
    await assert.rejects(
      async () => {
        await session.updateMetadata('nonexistent', { data: 'test' });
      },
      (err) => {
        assert.ok(err instanceof Error, 'Should throw Error');
        assert.ok(
          err.message.includes('not found') || err.message.includes('does not exist'),
          `Error message should mention not found, got: ${err.message}`
        );
        return true;
      },
      'Should throw error for non-existent vector'
    );
  } finally {
    await session.destroy();
  }
});

test('updateMetadata - preserves internal _originalId field', async (t) => {
  const session = await VectorDbSession.create(TEST_CONFIG);

  try {
    // Add training vectors
    await addTrainingVectors(session);

    // Add vector
    const vector = generateTestVector(7);
    await session.addVectors([{
      id: 'preserve-test',
      vector,
      metadata: { info: 'initial' },
    }]);

    // Update metadata
    await session.updateMetadata('preserve-test', {
      info: 'updated',
      extra: 'data',
    });

    // Internal verification: _originalId should still be preserved
    // This is checked internally by the binding
    const results = await session.search(vector, 20);
    const preserveResult = results.find(r => r.id === 'preserve-test');
    assert.ok(preserveResult, 'Should find preserve-test vector');
    assert.strictEqual(preserveResult.id, 'preserve-test', 'Should preserve original ID');
  } finally {
    await session.destroy();
  }
});

test('updateMetadata - update multiple vectors sequentially', async (t) => {
  const session = await VectorDbSession.create(TEST_CONFIG);

  try {
    // Add multiple vectors
    const vectors = [];
    for (let i = 0; i < 5; i++) {
      vectors.push({
        id: `multi-${i}`,
        vector: generateTestVector(i),
        metadata: { index: i, status: 'initial' },
      });
    }
    await session.addVectors(vectors);

    // Update each vector's metadata
    for (let i = 0; i < 5; i++) {
      await session.updateMetadata(`multi-${i}`, {
        index: i,
        status: 'updated',
        updateTime: Date.now(),
      });
    }

    // Verify all updates
    for (let i = 0; i < 5; i++) {
      const results = await session.search(generateTestVector(i), 20);
      const multiResult = results.find(r => r.id === `multi-${i}`);
      assert.ok(multiResult, `Should find multi-${i}`);
      assert.strictEqual(multiResult.metadata.status, 'updated', `Vector ${i} should be updated`);
      assert.strictEqual(multiResult.metadata.index, i);
      assert.ok(multiResult.metadata.updateTime, `Vector ${i} should have updateTime`);
    }
  } finally {
    await session.destroy();
  }
});

test('updateMetadata - with native object metadata (no stringify)', async (t) => {
  const session = await VectorDbSession.create(TEST_CONFIG);

  try {
    // Add training vectors
    await addTrainingVectors(session);

    // Add vector
    const vector = generateTestVector(9);
    await session.addVectors([{
      id: 'native-obj',
      vector,
      metadata: { initial: true },
    }]);

    // Update with complex native object
    const complexMetadata = {
      string: 'text',
      number: 42,
      float: 3.14,
      boolean: true,
      null: null,
      array: [1, 2, 3],
      nested: {
        deep: {
          value: 'nested',
        },
      },
    };

    await session.updateMetadata('native-obj', complexMetadata);

    // Verify native object preserved
    const results = await session.search(vector, 1);
    assert.deepStrictEqual(results[0].metadata, complexMetadata, 'Complex metadata should be preserved');
    assert.strictEqual(typeof results[0].metadata.nested, 'object', 'Nested should be object');
    assert.strictEqual(results[0].metadata.nested.deep.value, 'nested', 'Deep nesting preserved');
  } finally {
    await session.destroy();
  }
});

test('updateMetadata - update after load from S5', async (t) => {
  let session1 = await VectorDbSession.create(TEST_CONFIG);
  let cid;

  try {
    // Add training vectors
    await addTrainingVectors(session1);

    // Add vectors and save to S5
    await session1.addVectors([
      {
        id: 'persist1',
        vector: generateTestVector(10),
        metadata: { saved: true, version: 1 },
      },
      {
        id: 'persist2',
        vector: generateTestVector(11),
        metadata: { saved: true, version: 1 },
      },
    ]);

    cid = await session1.saveToS5();
    assert.ok(cid, 'Should get CID from save');
  } finally {
    await session1.destroy();
  }

  // Load in new session and update
  const session2 = await VectorDbSession.create({
    ...TEST_CONFIG,
    sessionId: 'test-session-update-metadata-reload',
  });

  try {
    await session2.loadUserVectors(cid, { lazyLoad: true });

    // Update metadata after load
    await session2.updateMetadata('persist1', {
      saved: true,
      version: 2,
      updated: 'after load',
    });

    // Verify update (search with k=20 to find among training vectors)
    const results = await session2.search(generateTestVector(10), 20);
    const persist1Result = results.find(r => r.id === 'persist1');
    assert.ok(persist1Result, 'Should find persist1');
    assert.strictEqual(persist1Result.metadata.version, 2, 'Version should be updated');
    assert.strictEqual(persist1Result.metadata.updated, 'after load', 'New field should exist');

    // Verify other vector unchanged
    const results2 = await session2.search(generateTestVector(11), 20);
    const persist2Result = results2.find(r => r.id === 'persist2');
    assert.ok(persist2Result, 'Should find persist2');
    assert.strictEqual(persist2Result.metadata.version, 1, 'Other vector should be unchanged');
  } finally {
    await session2.destroy();
  }
});

test('updateMetadata - update and save to S5', async (t) => {
  let session1 = await VectorDbSession.create(TEST_CONFIG);
  let cid;

  try {
    // Add training vectors
    await addTrainingVectors(session1);

    // Add vector
    await session1.addVectors([{
      id: 'update-save',
      vector: generateTestVector(15),
      metadata: { initial: 'data' },
    }]);

    // Update metadata
    await session1.updateMetadata('update-save', {
      updated: 'metadata',
      timestamp: Date.now(),
    });

    // Save to S5
    cid = await session1.saveToS5();
    assert.ok(cid, 'Should save successfully');
  } finally {
    await session1.destroy();
  }

  // Load in new session and verify
  const session2 = await VectorDbSession.create({
    ...TEST_CONFIG,
    sessionId: 'test-session-update-metadata-verify-save',
  });

  try {
    await session2.loadUserVectors(cid, { lazyLoad: true });

    const results = await session2.search(generateTestVector(15), 20);
    const updateSaveResult = results.find(r => r.id === 'update-save');
    assert.ok(updateSaveResult, 'Should find update-save');
    assert.strictEqual(updateSaveResult.metadata.updated, 'metadata', 'Updated metadata should persist');
    assert.strictEqual(updateSaveResult.metadata.initial, undefined, 'Old metadata should be replaced');
  } finally {
    await session2.destroy();
  }
});
