const { describe, test, before, after } = require('node:test');
const assert = require('node:assert');
const { VectorDbSession } = require('../index.js');
const { startS5Service } = require('./helpers/s5-service.cjs');

let s5Service = null;

before(async () => {
  console.log('Starting S5 service...');
  s5Service = await startS5Service({ port: 5529, mode: 'mock' });
  console.log('S5 service started on port 5529');
});

after(async () => {
  if (s5Service) {
    await s5Service.close();
  }
});

describe('Issue #4: $gt and $lt Operators', () => {
  let session;

  before(async () => {
    session = await VectorDbSession.create({
      s5Portal: 'http://127.0.0.1:5529',
      userSeedPhrase: 'gt-lt-test-seed',
      sessionId: 'gt-lt-session',
      encryptAtRest: false,
    });

    // Add test vectors with varying scores
    const vectors = [];
    for (let i = 0; i <= 100; i += 10) {
      vectors.push({
        id: `doc-score-${i}`,
        vector: Array(384).fill(0).map((_, idx) => Math.sin((idx + i) * 0.1)),
        metadata: {
          score: i,
          title: `Document with score ${i}`
        }
      });
    }
    await session.addVectors(vectors);
  });

  after(async () => {
    await session.destroy();
  });

  test('$gt: strictly greater than (excludes boundary)', async () => {
    const queryVector = Array(384).fill(0).map((_, idx) => Math.sin(idx * 0.1));

    // Search for scores strictly greater than 40 (should get 50, 60, 70, 80, 90, 100)
    const results = await session.search(queryVector, 20, {
      threshold: 0,
      filter: { score: { $gt: 40 } }
    });

    const scores = results.map(r => r.metadata.score).sort((a, b) => a - b);
    console.log('$gt 40 results:', scores);

    // Should NOT include 40
    assert.ok(!scores.includes(40), 'Should NOT include boundary value 40');

    // Should include values > 40
    assert.ok(scores.includes(50), 'Should include 50');
    assert.ok(scores.includes(60), 'Should include 60');
    assert.ok(scores.includes(100), 'Should include 100');

    // Should NOT include values <= 40
    assert.ok(!scores.includes(30), 'Should NOT include 30');
    assert.ok(!scores.includes(0), 'Should NOT include 0');

    assert.strictEqual(scores.length, 6, 'Should return exactly 6 results (50-100)');
  });

  test('$gte: greater than or equal (includes boundary)', async () => {
    const queryVector = Array(384).fill(0).map((_, idx) => Math.sin(idx * 0.1));

    // Search for scores >= 40 (should get 40, 50, 60, 70, 80, 90, 100)
    const results = await session.search(queryVector, 20, {
      threshold: 0,
      filter: { score: { $gte: 40 } }
    });

    const scores = results.map(r => r.metadata.score).sort((a, b) => a - b);
    console.log('$gte 40 results:', scores);

    // Should include 40 (boundary)
    assert.ok(scores.includes(40), 'Should include boundary value 40');
    assert.ok(scores.includes(50), 'Should include 50');
    assert.ok(scores.includes(100), 'Should include 100');

    assert.strictEqual(scores.length, 7, 'Should return exactly 7 results (40-100)');
  });

  test('$lt: strictly less than (excludes boundary)', async () => {
    const queryVector = Array(384).fill(0).map((_, idx) => Math.sin(idx * 0.1));

    // Search for scores strictly less than 50 (should get 0, 10, 20, 30, 40)
    const results = await session.search(queryVector, 20, {
      threshold: 0,
      filter: { score: { $lt: 50 } }
    });

    const scores = results.map(r => r.metadata.score).sort((a, b) => a - b);
    console.log('$lt 50 results:', scores);

    // Should NOT include 50
    assert.ok(!scores.includes(50), 'Should NOT include boundary value 50');

    // Should include values < 50
    assert.ok(scores.includes(0), 'Should include 0');
    assert.ok(scores.includes(10), 'Should include 10');
    assert.ok(scores.includes(40), 'Should include 40');

    // Should NOT include values >= 50
    assert.ok(!scores.includes(60), 'Should NOT include 60');
    assert.ok(!scores.includes(100), 'Should NOT include 100');

    assert.strictEqual(scores.length, 5, 'Should return exactly 5 results (0-40)');
  });

  test('$lte: less than or equal (includes boundary)', async () => {
    const queryVector = Array(384).fill(0).map((_, idx) => Math.sin(idx * 0.1));

    // Search for scores <= 50 (should get 0, 10, 20, 30, 40, 50)
    const results = await session.search(queryVector, 20, {
      threshold: 0,
      filter: { score: { $lte: 50 } }
    });

    const scores = results.map(r => r.metadata.score).sort((a, b) => a - b);
    console.log('$lte 50 results:', scores);

    // Should include 50 (boundary)
    assert.ok(scores.includes(50), 'Should include boundary value 50');
    assert.ok(scores.includes(0), 'Should include 0');
    assert.ok(scores.includes(40), 'Should include 40');

    assert.strictEqual(scores.length, 6, 'Should return exactly 6 results (0-50)');
  });

  test('$gt and $lt combined (exclusive range)', async () => {
    const queryVector = Array(384).fill(0).map((_, idx) => Math.sin(idx * 0.1));

    // Search for 20 < score < 70 (should get 30, 40, 50, 60)
    const results = await session.search(queryVector, 20, {
      threshold: 0,
      filter: { score: { $gt: 20, $lt: 70 } }
    });

    const scores = results.map(r => r.metadata.score).sort((a, b) => a - b);
    console.log('20 < score < 70 results:', scores);

    // Should NOT include boundaries
    assert.ok(!scores.includes(20), 'Should NOT include lower boundary 20');
    assert.ok(!scores.includes(70), 'Should NOT include upper boundary 70');

    // Should include values in between
    assert.ok(scores.includes(30), 'Should include 30');
    assert.ok(scores.includes(40), 'Should include 40');
    assert.ok(scores.includes(50), 'Should include 50');
    assert.ok(scores.includes(60), 'Should include 60');

    assert.strictEqual(scores.length, 4, 'Should return exactly 4 results (30-60)');
  });

  test('$gte and $lte combined (inclusive range)', async () => {
    const queryVector = Array(384).fill(0).map((_, idx) => Math.sin(idx * 0.1));

    // Search for 20 <= score <= 70 (should get 20, 30, 40, 50, 60, 70)
    const results = await session.search(queryVector, 20, {
      threshold: 0,
      filter: { score: { $gte: 20, $lte: 70 } }
    });

    const scores = results.map(r => r.metadata.score).sort((a, b) => a - b);
    console.log('20 <= score <= 70 results:', scores);

    // Should include boundaries
    assert.ok(scores.includes(20), 'Should include lower boundary 20');
    assert.ok(scores.includes(70), 'Should include upper boundary 70');

    // Should include values in between
    assert.ok(scores.includes(30), 'Should include 30');
    assert.ok(scores.includes(50), 'Should include 50');
    assert.ok(scores.includes(60), 'Should include 60');

    assert.strictEqual(scores.length, 6, 'Should return exactly 6 results (20-70)');
  });

  test('mixed inclusive/exclusive range: $gte and $lt', async () => {
    const queryVector = Array(384).fill(0).map((_, idx) => Math.sin(idx * 0.1));

    // Search for 30 <= score < 80 (should get 30, 40, 50, 60, 70)
    const results = await session.search(queryVector, 20, {
      threshold: 0,
      filter: { score: { $gte: 30, $lt: 80 } }
    });

    const scores = results.map(r => r.metadata.score).sort((a, b) => a - b);
    console.log('30 <= score < 80 results:', scores);

    // Should include lower boundary, exclude upper boundary
    assert.ok(scores.includes(30), 'Should include lower boundary 30');
    assert.ok(!scores.includes(80), 'Should NOT include upper boundary 80');

    assert.ok(scores.includes(40), 'Should include 40');
    assert.ok(scores.includes(70), 'Should include 70');

    assert.strictEqual(scores.length, 5, 'Should return exactly 5 results (30-70)');
  });

  test('mixed inclusive/exclusive range: $gt and $lte', async () => {
    const queryVector = Array(384).fill(0).map((_, idx) => Math.sin(idx * 0.1));

    // Search for 30 < score <= 80 (should get 40, 50, 60, 70, 80)
    const results = await session.search(queryVector, 20, {
      threshold: 0,
      filter: { score: { $gt: 30, $lte: 80 } }
    });

    const scores = results.map(r => r.metadata.score).sort((a, b) => a - b);
    console.log('30 < score <= 80 results:', scores);

    // Should exclude lower boundary, include upper boundary
    assert.ok(!scores.includes(30), 'Should NOT include lower boundary 30');
    assert.ok(scores.includes(80), 'Should include upper boundary 80');

    assert.ok(scores.includes(40), 'Should include 40');
    assert.ok(scores.includes(70), 'Should include 70');

    assert.strictEqual(scores.length, 5, 'Should return exactly 5 results (40-80)');
  });
});
