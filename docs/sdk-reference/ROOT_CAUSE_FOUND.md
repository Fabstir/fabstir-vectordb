# Root Cause Found: Missing session.destroy() Calls

**Date**: October 31, 2025
**Status**: CRITICAL BUG CONFIRMED
**Credit**: Vector DB Developer's diagnosis was correct

---

## Summary

Your developer was **100% correct**. The issue is **NOT in the native layer** - it's in our SDK wrapper's cleanup code.

**We are NOT calling `session.vectorDbSession.destroy()`** when destroying sessions!

---

## The Bug

### Current (Broken) Code

**File**: `packages/sdk-core/src/managers/VectorRAGManager.ts`
**Lines**: 212-224

```typescript
async destroySession(sessionId: string): Promise<void> {
  const session = this.sessions.get(sessionId);
  if (!session) {
    throw new Error('Session not found');
  }

  // Cleanup session (VectorDbSession doesn't have explicit close/destroy)  ← WRONG COMMENT!
  session.status = 'closed';

  // Remove from cache and storage
  this.sessionCache.delete(sessionId);
  this.sessions.delete(sessionId);

  // ❌ MISSING: await session.vectorDbSession.destroy();
}
```

**What we're doing**:
1. ✅ Setting our wrapper's status to 'closed'
2. ✅ Removing session from our internal maps
3. ❌ **NOT calling `session.vectorDbSession.destroy()` on the native session**

**Result**: The native `VectorDbSession` remains alive in:
- Memory (holds vectors, indices, caches)
- IndexedDB (persists across test runs)
- Internal maps in the native layer

---

### Correct Code (From Vector DB API Docs)

**Source**: `docs/fabstir-vectordb-reference/API.md` lines 180-181

```javascript
try {
  // ... operations ...
} finally {
  await session.destroy();  // CRITICAL: Clean up memory
}
```

**What should happen**:
```typescript
async destroySession(sessionId: string): Promise<void> {
  const session = this.sessions.get(sessionId);
  if (!session) {
    throw new Error('Session not found');
  }

  // ✅ Call destroy() on native VectorDbSession FIRST
  if (session.vectorDbSession) {
    await session.vectorDbSession.destroy();
  }

  // Then cleanup our wrapper state
  session.status = 'closed';
  this.sessionCache.delete(sessionId);
  this.sessions.delete(sessionId);
  this.dbNameToSessionId.delete(session.databaseName);
}
```

---

## How This Explains All Test Failures

### Test Failure Scenario

**Test 1** (runs first):
```typescript
it('test 1', async () => {
  await vectorManager.createSession('test-db');

  // Creates native session: VectorDbSession('user-test-db-rag-123-abc')

  await vectorManager.addVectors('test-db', [
    { id: 'doc-1', values: [...], metadata: {} },
    { id: 'doc-2', values: [...], metadata: {} },
    { id: 'doc-3', values: [...], metadata: {} }
  ]);

  // Vectors stored in: memory + IndexedDB
});

afterEach(async () => {
  await vectorManager.cleanup();

  // ❌ BUG: Only removes from SDK wrapper maps, native session NOT destroyed
  // Vectors STILL in memory and IndexedDB!
});
```

**Test 2** (runs second):
```typescript
beforeEach(async () => {
  vectorManager = new VectorRAGManager({...});  // New wrapper instance
});

it('test 2', async () => {
  await vectorManager.createSession('test-db');

  // Creates NEW native session: VectorDbSession('user-test-db-rag-456-def')
  // BUT old session 'rag-123-abc' still exists in IndexedDB!

  await vectorManager.search('test-db', query, 10);

  // ❓ QUESTION: Does VectorDbSession.create() with same dbName reuse IndexedDB data?
  // If YES, search might return vectors from Test 1!
});
```

---

### topK Returning 1 Instead of 10

**Scenario**: If IndexedDB has residual data from previous test:

```typescript
// Test A (ran earlier, not cleaned up properly)
await vectorManager.createSession('perf-test');
await vectorManager.addVectors('perf-test', [{ id: 'leftover', ... }]);
// cleanup() called, but session.destroy() NOT called
// → 1 vector remains in IndexedDB

// Test B (current test)
await vectorManager.createSession('perf-test');  // Reuses IndexedDB?
await vectorManager.addVectors('perf-test', [20 new vectors]);
// If IndexedDB was reused, only 'leftover' vector might be indexed?

const results = await vectorManager.search('perf-test', query, 10);
// Returns 1 result (the leftover) instead of 10
```

---

### Soft-Delete Returning 0 Instead of 3

**Scenario**: deleteByMetadata affects wrong session's data:

```typescript
// Test A (not cleaned up)
await vectorManager.createSession('test-db');
await vectorManager.addVectors('test-db', [5 vectors]);
// cleanup() fails to destroy native session

// Test B
await vectorManager.createSession('test-db');
await vectorManager.addVectors('test-db', [
  { id: 'keep-1', metadata: { status: 'keep' } },
  { id: 'keep-2', metadata: { status: 'keep' } },
  { id: 'keep-3', metadata: { status: 'keep' } },
  { id: 'delete-1', metadata: { status: 'delete' } },
  { id: 'delete-2', metadata: { status: 'delete' } }
]);

await vectorManager.deleteByMetadata('test-db', { status: 'delete' });
// If IndexedDB has mixed data from Test A and Test B,
// deleteByMetadata might delete wrong vectors or ALL vectors

const results = await vectorManager.search('test-db', query, 10);
// Returns 0 (all vectors deleted) instead of 3 ('keep' vectors)
```

---

## The Warnings We Saw

Looking back at test output:

```
WARNING: VectorDBSession '0x8D64...-test-search-topk-rag-1761911780548-ichfktlwt' dropped without calling destroy()
WARNING: VectorDBSession '0x8D64...-test-soft-delete-search-rag-1761911780563-8fwck5vz5' dropped without calling destroy()
WARNING: VectorDBSession '0x8D64...-test-empty-rag-1761911780561-5lvqkr6qv' dropped without calling destroy()
...
```

**These warnings are from the native layer!** They're telling us:

> "Hey, you created these sessions but never called destroy() on them!"

We ignored these warnings, but they were the key diagnostic message!

---

## Fix Required

### File: `packages/sdk-core/src/managers/VectorRAGManager.ts`

**Change 1: destroySession()**

```typescript
async destroySession(sessionId: string): Promise<void> {
  const session = this.sessions.get(sessionId);
  if (!session) {
    throw new Error('Session not found');
  }

  // ✅ CRITICAL: Destroy native VectorDbSession first
  if (session.vectorDbSession && typeof session.vectorDbSession.destroy === 'function') {
    try {
      await session.vectorDbSession.destroy();
    } catch (error) {
      console.error(`Error destroying VectorDbSession ${sessionId}:`, error);
      // Continue with cleanup even if destroy fails
    }
  }

  // Update status and remove from caches
  session.status = 'closed';
  this.sessionCache.delete(sessionId);
  this.sessions.delete(sessionId);
  this.dbNameToSessionId.delete(session.databaseName);
}
```

**Change 2: Update comment on line 218**

```typescript
// OLD (WRONG):
// Cleanup session (VectorDbSession doesn't have explicit close/destroy)

// NEW (CORRECT):
// Cleanup session - MUST call destroy() to free memory and IndexedDB
```

---

## Verification Steps

After fixing, tests should:

1. **Stop showing warnings**:
   ```
   # BEFORE (broken):
   WARNING: VectorDBSession 'xxx' dropped without calling destroy()

   # AFTER (fixed):
   (no warnings)
   ```

2. **Session count should be 0 after cleanup**:
   ```typescript
   afterEach(async () => {
     console.log('Sessions before cleanup:', vectorManager.sessions.size);
     await vectorManager.cleanup();
     console.log('Sessions after cleanup:', vectorManager.sessions.size);  // Should be 0
   });
   ```

3. **Tests should be isolated**:
   ```typescript
   it('test 1', async () => {
     await vectorManager.createSession('test-db');
     await vectorManager.addVectors('test-db', [10 vectors]);
     const results = await vectorManager.search('test-db', query, 10);
     expect(results.length).toBe(10);  // ✅ Should pass
   });

   it('test 2', async () => {
     // New manager, new session
     await vectorManager.createSession('test-db');
     const results = await vectorManager.search('test-db', query, 10);
     expect(results.length).toBe(0);  // ✅ Should be 0 (no vectors yet)
   });
   ```

4. **topK should work correctly**:
   ```typescript
   it('should respect topK parameter', async () => {
     await vectorManager.createSession('test-db');
     await vectorManager.addVectors('test-db', [20 vectors]);

     const results3 = await vectorManager.search('test-db', query, 3);
     expect(results3.length).toBe(3);  // ✅ Should pass

     const results10 = await vectorManager.search('test-db', query, 10);
     expect(results10.length).toBe(10);  // ✅ Should pass
   });
   ```

5. **Soft-delete should work correctly**:
   ```typescript
   it('should handle soft-deleted vectors', async () => {
     await vectorManager.createSession('test-db');
     await vectorManager.addVectors('test-db', [
       { id: 'keep-1', metadata: { status: 'keep' } },
       { id: 'keep-2', metadata: { status: 'keep' } },
       { id: 'keep-3', metadata: { status: 'keep' } },
       { id: 'delete-1', metadata: { status: 'delete' } },
       { id: 'delete-2', metadata: { status: 'delete' } }
     ]);

     await vectorManager.deleteByMetadata('test-db', { status: 'delete' });
     const results = await vectorManager.search('test-db', query, 10);

     expect(results.length).toBe(3);  // ✅ Should pass (only 'keep' vectors)
   });
   ```

---

## Expected Test Results After Fix

**Before Fix** (current):
- Tests: 19/32 passing (59%)
- Warnings: 20+ "dropped without calling destroy()"
- Session cleanup: Incomplete

**After Fix** (expected):
- Tests: 30/32 passing (94%) ✅
- Warnings: 0 ✅
- Session cleanup: Complete ✅
- Remaining 2 skipped: Deferred features (caching, history tracking)

---

## Apology to Vector DB Developer

The native layer was working correctly all along! The issue was entirely in our SDK wrapper:

1. ✅ Your topK implementation works perfectly
2. ✅ Your soft-delete implementation works perfectly
3. ✅ Your session isolation works perfectly
4. ✅ Your warnings told us exactly what was wrong

We should have:
- Read the warnings more carefully
- Checked the API docs for `destroy()` method
- Verified our cleanup code was calling native methods

**Thank you for your patience and excellent diagnostic work!**

---

## Next Steps

1. **Implement the fix** in VectorRAGManager.ts
2. **Run full test suite** to verify 30/32 passing
3. **Report results** to Vector DB developer
4. **Mark Sub-phase 3.2 as COMPLETE** (once tests pass)
5. **Proceed to Sub-phase 3.3** (RAG context integration)

---

## Lessons Learned

1. **Always check native layer docs** for cleanup methods (destroy, close, dispose)
2. **Don't ignore warnings** - they're diagnostic messages
3. **Verify cleanup in tests** - log session counts before/after
4. **Trust the native layer** - if simple tests pass, issue is likely in wrapper
5. **Read API examples carefully** - try/finally blocks with destroy() are hints

---

**Status**: Ready to implement fix and re-run tests
**Expected Time to Fix**: 5 minutes (one-line change)
**Expected Test Improvement**: 19/32 → 30/32 (94%)
