const { describe, test, before, after } = require('node:test');
const assert = require('node:assert');
const { VectorDbSession } = require('../index.js');
const { startS5Service } = require('./helpers/s5-service.cjs');

// S5 service instance
let s5Service = null;

before(async () => {
  console.log('Starting S5 service...');
  s5Service = await startS5Service({ port: 5527, mode: 'mock' });
  console.log('S5 service started on port 5527');
});

after(async () => {
  if (s5Service) {
    await s5Service.close();
  }
});

describe('Bug: includeVectors Option Not Working', () => {
  let session;

  before(async () => {
    session = await VectorDbSession.create({
      s5Portal: 'http://127.0.0.1:5527',
      userSeedPhrase: 'include-vectors-test-seed',
      sessionId: 'include-vectors-session',
      encryptAtRest: false,
    });

    // Add test vectors
    const vectors = [];
    for (let i = 0; i < 10; i++) {
      vectors.push({
        id: `doc-${i}`,
        vector: Array(384).fill(0).map((_, idx) => Math.sin((idx + i) * 0.1)),
        metadata: { index: i, text: `Document ${i}` }
      });
    }
    await session.addVectors(vectors);
  });

  after(async () => {
    await session.destroy();
  });

  test('should NOT include vectors by default', async () => {
    const queryVector = Array(384).fill(0).map((_, idx) => Math.sin(idx * 0.1));
    const results = await session.search(queryVector, 5, { threshold: 0 });

    assert.ok(results.length > 0, 'Should return at least one result');
    assert.strictEqual(results[0].vector, undefined, 'Vector should be undefined by default');
  });

  test('should include vectors when includeVectors = true', async () => {
    const queryVector = Array(384).fill(0).map((_, idx) => Math.sin(idx * 0.1));
    const results = await session.search(queryVector, 5, { threshold: 0, includeVectors: true });

    assert.ok(results.length > 0, 'Should return at least one result');
    assert.ok(Array.isArray(results[0].vector), 'Vector should be an array');
    assert.strictEqual(results[0].vector.length, 384, 'Vector should have 384 dimensions');

    // Check that all results have vectors
    for (const result of results) {
      assert.ok(Array.isArray(result.vector), 'Each result should have a vector');
      assert.strictEqual(result.vector.length, 384, 'Each vector should have 384 dimensions');
    }
  });

  test('should NOT include vectors when includeVectors = false', async () => {
    const queryVector = Array(384).fill(0).map((_, idx) => Math.sin(idx * 0.1));
    const results = await session.search(queryVector, 5, { threshold: 0, includeVectors: false });

    assert.ok(results.length > 0, 'Should return at least one result');
    assert.strictEqual(results[0].vector, undefined, 'Vector should be undefined when includeVectors is false');
  });

  test('should return correct vector values', async () => {
    const queryVector = Array(384).fill(0).map((_, idx) => Math.sin(idx * 0.1));
    const results = await session.search(queryVector, 1, { threshold: 0, includeVectors: true });

    assert.ok(results.length === 1, 'Should return exactly one result');
    const vector = results[0].vector;

    // Check first few values match expected pattern
    assert.ok(Math.abs(vector[0] - Math.sin(0 * 0.1)) < 0.01, 'First value should match');
    assert.ok(Math.abs(vector[1] - Math.sin(1 * 0.1)) < 0.01, 'Second value should match');
    assert.ok(Math.abs(vector[2] - Math.sin(2 * 0.1)) < 0.01, 'Third value should match');
  });
});
