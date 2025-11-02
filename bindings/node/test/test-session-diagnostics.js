const { describe, test } = require('node:test');
const assert = require('node:assert');
const { VectorDbSession } = require('../index.js');

describe('Session State Diagnostics', () => {

  test('Diagnostic 1: Session creation with same sessionId', async () => {
    console.log('\n=== DIAGNOSTIC 1: Session ID Reuse Test ===');

    // Create session 1
    console.log('[1] Creating session1 with sessionId="test-session-123"...');
    const session1 = await VectorDbSession.create({
      s5Portal: 'http://127.0.0.1:5520',
      userSeedPhrase: 'test-seed',
      sessionId: 'test-session-123',
      encryptAtRest: false
    });

    // Add 1 vector to session1
    console.log('[2] Adding 1 vector to session1...');
    await session1.addVectors([{
      id: 'doc-1',
      vector: new Array(384).fill(0).map((_, i) => i / 384),
      metadata: { source: 'session1' }
    }]);

    // Search session1 - should return 1 result
    const results1 = await session1.search(new Array(384).fill(0).map((_, i) => i / 384), 10, { threshold: 0 });
    console.log(`[3] Session1 search: ${results1.length} results (expected 1)`);
    assert.strictEqual(results1.length, 1, 'Session1 should have 1 vector');

    // Create session 2 with SAME sessionId
    console.log('[4] Creating session2 with SAME sessionId="test-session-123"...');
    const session2 = await VectorDbSession.create({
      s5Portal: 'http://127.0.0.1:5520',
      userSeedPhrase: 'test-seed',
      sessionId: 'test-session-123',  // ← SAME ID
      encryptAtRest: false
    });

    // Search session2 without adding vectors
    const results2 = await session2.search(new Array(384).fill(0).map((_, i) => i / 384), 10, { threshold: 0 });
    console.log(`[5] Session2 search: ${results2.length} results`);

    if (results2.length === 0) {
      console.log('  ✅ CORRECT: Session2 is isolated (new session created)');
    } else if (results2.length === 1) {
      console.log('  ❌ BUG FOUND: Session2 reused session1 data!');
      console.log(`  Result ID: ${results2[0].id}, Source: ${results2[0].metadata.source}`);
    } else {
      console.log(`  ⚠️ UNEXPECTED: Session2 has ${results2.length} results`);
    }

    assert.strictEqual(results2.length, 0, 'Session2 should be empty (new session)');

    await session1.destroy();
    await session2.destroy();
  });

  test('Diagnostic 2: Session ID parsing/collision', async () => {
    console.log('\n=== DIAGNOSTIC 2: Session ID Collision Test ===');

    // Create sessions with similar IDs (same prefix, different suffix)
    console.log('[1] Creating session with ID="user-db-rag-123-abc"...');
    const session1 = await VectorDbSession.create({
      s5Portal: 'http://127.0.0.1:5520',
      userSeedPhrase: 'test-seed',
      sessionId: 'user-db-rag-123-abc',
      encryptAtRest: false
    });

    await session1.addVectors([{
      id: 'doc-abc',
      vector: new Array(384).fill(0).map(() => Math.random()),
      metadata: { suffix: 'abc' }
    }]);

    console.log('[2] Creating session with ID="user-db-rag-123-xyz"...');
    const session2 = await VectorDbSession.create({
      s5Portal: 'http://127.0.0.1:5520',
      userSeedPhrase: 'test-seed',
      sessionId: 'user-db-rag-123-xyz',  // Same timestamp part, different suffix
      encryptAtRest: false
    });

    await session2.addVectors([{
      id: 'doc-xyz',
      vector: new Array(384).fill(0).map(() => Math.random()),
      metadata: { suffix: 'xyz' }
    }]);

    // Verify isolation
    const results1 = await session1.search(new Array(384).fill(0).map(() => Math.random()), 10, { threshold: 0 });
    const results2 = await session2.search(new Array(384).fill(0).map(() => Math.random()), 10, { threshold: 0 });

    console.log(`[3] Session1 results: ${results1.length}, IDs: ${results1.map(r => r.id).join(', ')}`);
    console.log(`[4] Session2 results: ${results2.length}, IDs: ${results2.map(r => r.id).join(', ')}`);

    // Check for cross-contamination
    const session1HasXyz = results1.some(r => r.id === 'doc-xyz');
    const session2HasAbc = results2.some(r => r.id === 'doc-abc');

    if (session1HasXyz || session2HasAbc) {
      console.log('  ❌ BUG FOUND: Sessions are contaminated!');
      if (session1HasXyz) console.log('    Session1 has doc-xyz (from session2)');
      if (session2HasAbc) console.log('    Session2 has doc-abc (from session1)');
    } else {
      console.log('  ✅ CORRECT: Sessions are properly isolated');
    }

    assert.ok(!session1HasXyz, 'Session1 should not have session2 data');
    assert.ok(!session2HasAbc, 'Session2 should not have session1 data');

    await session1.destroy();
    await session2.destroy();
  });

  test('Diagnostic 3: Rapid session creation (concurrency)', async () => {
    console.log('\n=== DIAGNOSTIC 3: Rapid Session Creation Test ===');

    const sessions = [];
    const sessionCount = 10;

    console.log(`[1] Creating ${sessionCount} sessions rapidly...`);

    // Create sessions concurrently (might share same timestamp)
    for (let i = 0; i < sessionCount; i++) {
      const sessionId = `rapid-test-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
      const session = await VectorDbSession.create({
        s5Portal: 'http://127.0.0.1:5520',
        userSeedPhrase: 'test-seed',
        sessionId,
        encryptAtRest: false
      });

      // Add unique vector to each session
      await session.addVectors([{
        id: `doc-${i}`,
        vector: new Array(384).fill(0).map(() => Math.random()),
        metadata: { index: i }
      }]);

      sessions.push({ session, index: i });
    }

    console.log(`[2] Created ${sessions.length} sessions`);

    // Verify each session has only its own vector
    console.log('[3] Verifying session isolation...');
    let contaminated = 0;
    let correct = 0;

    for (const { session, index } of sessions) {
      const results = await session.search(new Array(384).fill(0).map(() => Math.random()), 10, { threshold: 0 });

      const expectedId = `doc-${index}`;
      const hasOwnVector = results.some(r => r.id === expectedId);
      const hasOtherVectors = results.some(r => r.id !== expectedId);

      if (!hasOwnVector || hasOtherVectors) {
        contaminated++;
        console.log(`  ❌ Session ${index}: expected only ${expectedId}, got ${results.map(r => r.id).join(', ')}`);
      } else {
        correct++;
      }
    }

    console.log(`[4] Results: ${correct}/${sessionCount} sessions correctly isolated`);

    if (contaminated > 0) {
      console.log(`  ❌ BUG FOUND: ${contaminated} sessions have contaminated data`);
    } else {
      console.log('  ✅ CORRECT: All sessions properly isolated');
    }

    assert.strictEqual(contaminated, 0, `${contaminated} sessions had contaminated data`);

    // Cleanup
    for (const { session } of sessions) {
      await session.destroy();
    }
  });
});
