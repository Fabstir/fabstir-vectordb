const { describe, test, before, after } = require('node:test');
const assert = require('node:assert');
const { VectorDbSession } = require('../index.js');
const { startS5Service } = require('./helpers/s5-service.cjs');

let s5Service = null;

before(async () => {
  console.log('Starting S5 service...');
  s5Service = await startS5Service({ port: 5530, mode: 'mock' });
  console.log('S5 service started on port 5530');
});

after(async () => {
  if (s5Service) {
    await s5Service.close();
  }
});

describe('Issue #5: Query Dimension Mismatch Should Throw Error', () => {
  let session;

  before(async () => {
    session = await VectorDbSession.create({
      s5Portal: 'http://127.0.0.1:5530',
      userSeedPhrase: 'dimension-test-seed',
      sessionId: 'dimension-session',
      encryptAtRest: false,
    });

    // Add vectors with 384 dimensions (standard for all-MiniLM-L6-v2)
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

  test('should throw error when query dimension is too large', async () => {
    // Index has 384-dim vectors, query with 512-dim
    const wrongQuery = Array(512).fill(0).map((_, idx) => Math.sin(idx * 0.1));

    let errorThrown = false;
    let errorMessage = '';

    try {
      await session.search(wrongQuery, 10, { threshold: 0 });
    } catch (error) {
      errorThrown = true;
      errorMessage = error.message;
      console.log('Error caught (too large):', errorMessage);
    }

    assert.ok(errorThrown, 'Should throw an error for dimension mismatch');
    assert.ok(errorMessage.includes('dimension mismatch'), 'Error should mention dimension mismatch');
    assert.ok(errorMessage.includes('384'), 'Error should mention expected dimension (384)');
    assert.ok(errorMessage.includes('512'), 'Error should mention actual dimension (512)');
  });

  test('should throw error when query dimension is too small', async () => {
    // Index has 384-dim vectors, query with 128-dim
    const wrongQuery = Array(128).fill(0).map((_, idx) => Math.sin(idx * 0.1));

    let errorThrown = false;
    let errorMessage = '';

    try {
      await session.search(wrongQuery, 10, { threshold: 0 });
    } catch (error) {
      errorThrown = true;
      errorMessage = error.message;
      console.log('Error caught (too small):', errorMessage);
    }

    assert.ok(errorThrown, 'Should throw an error for dimension mismatch');
    assert.ok(errorMessage.includes('dimension mismatch'), 'Error should mention dimension mismatch');
    assert.ok(errorMessage.includes('384'), 'Error should mention expected dimension (384)');
    assert.ok(errorMessage.includes('128'), 'Error should mention actual dimension (128)');
  });

  test('should succeed when query dimension matches index dimension', async () => {
    // Index has 384-dim vectors, query with 384-dim (correct!)
    const correctQuery = Array(384).fill(0).map((_, idx) => Math.sin(idx * 0.1));

    let results;
    let errorThrown = false;

    try {
      results = await session.search(correctQuery, 10, { threshold: 0 });
    } catch (error) {
      errorThrown = true;
      console.error('Unexpected error:', error.message);
    }

    assert.ok(!errorThrown, 'Should NOT throw error when dimensions match');
    assert.ok(Array.isArray(results), 'Should return an array of results');
    assert.ok(results.length > 0, 'Should return at least one result');
  });

  test('should provide clear error message format', async () => {
    const wrongQuery = Array(256).fill(0).map((_, idx) => Math.sin(idx * 0.1));

    try {
      await session.search(wrongQuery, 10);
      assert.fail('Should have thrown an error');
    } catch (error) {
      console.log('Full error message:', error.message);

      // Verify error message has the expected format:
      // "Query vector dimension mismatch: expected 384 dimensions, got 256"
      assert.ok(error.message.toLowerCase().includes('query'), 'Error should mention "query"');
      assert.ok(error.message.toLowerCase().includes('vector'), 'Error should mention "vector"');
      assert.ok(error.message.toLowerCase().includes('dimension'), 'Error should mention "dimension"');
      assert.ok(error.message.includes('expected'), 'Error should mention "expected"');
      assert.ok(error.message.includes('got'), 'Error should mention "got"');
      assert.ok(error.message.includes('384'), 'Error should show expected dimension');
      assert.ok(error.message.includes('256'), 'Error should show actual dimension');
    }
  });

  test('should work correctly with filters even when checking dimensions', async () => {
    // Verify dimension check happens before filter processing
    const wrongQuery = Array(200).fill(0).map((_, idx) => Math.sin(idx * 0.1));

    try {
      await session.search(wrongQuery, 10, {
        threshold: 0,
        filter: { index: { $gte: 5 } }
      });
      assert.fail('Should have thrown dimension error before processing filter');
    } catch (error) {
      // Should get dimension error, NOT filter error
      assert.ok(error.message.includes('dimension'), 'Should fail on dimension check, not filter');
      assert.ok(!error.message.includes('filter'), 'Should not mention filter in error');
    }
  });
});
