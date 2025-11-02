# Session State Analysis - Response to Developer

**Date**: October 31, 2025
**Re**: Your finding about session isolation issues

---

## Your Questions Answered

### 1. Does VectorRAGManager create new sessions per database or reuse them?

**Answer**: It creates **NEW sessions each time**, but with potential **session ID collision issues**.

**Code Analysis** (`VectorRAGManager.ts` lines 79-123):

```typescript
async createSession(databaseName: string, config?: PartialRAGConfig): Promise<string> {
  // Generate unique session ID
  const sessionId = `rag-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;

  try {
    // Create VectorDbSession with unique sessionId
    const vectorDbSession = await VectorDbSession.create({
      s5Portal: sessionConfig.s5Portal,
      userSeedPhrase: this.seedPhrase,
      sessionId: `${this.userAddress}-${databaseName}-${sessionId}`,  // ← This is unique
      encryptAtRest: sessionConfig.encryptAtRest,
      chunkSize: sessionConfig.chunkSize,
      cacheSizeMb: sessionConfig.cacheSizeMb
    });

    // Store session mapping
    this.sessions.set(sessionId, session);
    this.sessionCache.set(sessionId, session);
    this.dbNameToSessionId.set(databaseName, sessionId); // ← Map dbName to sessionId

    return sessionId;
  }
}
```

**What happens**:
- Each call to `createSession('test-db')` creates a NEW `VectorDbSession` with a unique sessionId
- The sessionId format: `{userAddress}-{databaseName}-rag-{timestamp}-{random}`
- Example: `0x8D64...4bF6-test-db-rag-1761920951096-hjabntk2a`

**Potential Issue**: The `dbNameToSessionId` mapping gets **overwritten** if the same dbName is used twice:

```typescript
// Test 1
await vectorManager.createSession('test-db'); // sessionId: 'rag-123-abc'
// dbNameToSessionId.set('test-db', 'rag-123-abc')

await vectorManager.addVectors('test-db', vectors1); // Adds to session 'rag-123-abc'

// Test 2 (if not properly cleaned up)
await vectorManager.createSession('test-db'); // sessionId: 'rag-456-def'
// dbNameToSessionId.set('test-db', 'rag-456-def')  ← OVERWRITES previous mapping

await vectorManager.search('test-db', query, 10); // Uses session 'rag-456-def' (NEW, EMPTY)
// Returns 0 results because session is new and has no vectors!
```

---

### 2. Are sessions properly isolated between tests?

**Answer**: **YES**, tests create new VectorRAGManager instances and cleanup properly.

**Test Setup** (`basic-search.test.ts` lines 16-35):

```typescript
describe('Basic Vector Search', () => {
  let vectorManager: any;

  beforeEach(async () => {
    // Create NEW VectorRAGManager instance for each test
    vectorManager = new VectorRAGManager({
      userAddress: testUserAddress,
      seedPhrase: testSeedPhrase,
      config: DEFAULT_RAG_CONFIG
    });
  });

  afterEach(async () => {
    if (vectorManager) {
      await vectorManager.cleanup();  // Destroys ALL sessions
    }
  });

  it('test 1', async () => {
    await vectorManager.createSession('test-db');
    // ...
  });

  it('test 2', async () => {
    // NEW vectorManager instance (from beforeEach)
    await vectorManager.createSession('test-db');
    // ...
  });
});
```

**Cleanup code** (`VectorRAGManager.ts` lines 492-507):

```typescript
async cleanup(): Promise<void> {
  await this.dispose();
}

async dispose(): Promise<void> {
  if (this.disposed) return;

  await this.destroyAllSessions();  // Destroys ALL sessions
  this.sessionCache.clear();
  this.disposed = true;
}

async destroyAllSessions(): Promise<void> {
  const sessionIds = Array.from(this.sessions.keys());
  for (const sessionId of sessionIds) {
    await this.destroySession(sessionId);
  }
}
```

**Verdict**: SDK layer isolation is **correct** - each test gets a fresh `VectorRAGManager` instance.

---

### 3. What's the default threshold when calling search()?

**Answer**: **NO default threshold** - options are passed through as-is.

**Code** (`VectorRAGManager.ts` lines 420-424):

```typescript
async search(dbName: string, queryVector: number[], topK: number, options?: SearchOptions): Promise<any[]> {
  const sessionId = this.dbNameToSessionId.get(dbName);
  if (!sessionId) throw new Error('Session not found');
  return this.searchVectors(sessionId, queryVector, topK, options);  // ← No default options added
}

async searchVectors(
  sessionId: string,
  queryVector: number[],
  topK: number,
  options?: SearchOptions  // ← Optional, no defaults
): Promise<SearchResult[]> {
  const session = this.getSession(sessionId);
  if (!session) throw new Error('Session not found');
  if (session.status !== 'active') throw new Error('Session is closed');

  // Pass options directly to native VectorDbSession
  const results = await session.vectorDbSession.search(
    queryVector,
    topK,
    options  // ← Passed as-is to native layer
  );

  return results;
}
```

**Test calls**:
```typescript
// Most tests call WITHOUT options (no threshold)
const results = await vectorManager.search(dbName, query, 10);
// Equivalent to: vectorDbSession.search(query, 10, undefined)

// Some tests call WITH explicit threshold
const results = await vectorManager.search(dbName, query, 10, { threshold: 0.5 });
```

**Verdict**: SDK wrapper adds **no default threshold**. If native layer has a hidden default, that's in the Vector DB itself.

---

## Your Key Finding: Session Reuse Issue

**Your observation**:
> "Soft-delete test initially returned 10 results instead of 5"
> "Session was reusing vectors from previous test in same session"

**This is CRITICAL** - it suggests the **native VectorDbSession layer** is reusing sessions!

### Hypothesis: Native Layer Session Persistence

When you call `VectorDbSession.create()` with a sessionId that was previously used, what happens?

**Option A (Expected)**: Creates a **new** session, discarding any old data
```rust
// In Rust bindings
pub async fn create(options: SessionOptions) -> Result<VectorDbSession> {
    // Always create fresh session, even if sessionId exists
    let session = VectorDbSession::new(options);
    // ...
}
```

**Option B (Your Finding)**: **Reuses existing** session if sessionId matches
```rust
pub async fn create(options: SessionOptions) -> Result<VectorDbSession> {
    // Check if session with this ID already exists
    if let Some(existing) = SESSIONS.get(&options.session_id) {
        return Ok(existing);  // ← Reuse old session!
    }
    // ...
}
```

**If Option B is true**, this explains:
- Your test got 10 results instead of 5 (vectors from previous test persisted)
- Our topK test might get 1 result if previous test left a single vector in session

---

## The Smoking Gun: Session ID Format

Our sessionId format includes **timestamp + random string**:
```typescript
const sessionId = `rag-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
// Example: rag-1761920951096-hjabntk2a
```

This should be **globally unique** across all tests. BUT, if tests run **very fast** (within same millisecond), `Date.now()` could return the same value!

**Test 1** (at timestamp 1761920951096):
```
sessionId = rag-1761920951096-abc123
```

**Test 2** (at timestamp 1761920951096 - same millisecond):
```
sessionId = rag-1761920951096-xyz789  // Different random part, BUT...
```

If native layer **ignores the random part** and only looks at timestamp, it might treat these as the same session!

---

## Questions for You (Native Layer Developer)

### 1. Session Creation Behavior

When `VectorDbSession.create()` is called with a sessionId:

**Does it**:
- A) Always create a new session (discard any existing session with same ID)?
- B) Reuse existing session if one exists with that ID?
- C) Throw error if session already exists?

**Test this**:
```javascript
// Create session 1
const session1 = await VectorDbSession.create({
  s5Portal: 'https://s5.cx',
  userSeedPhrase: 'test',
  sessionId: 'test-session-123',
  encryptAtRest: false
});

await session1.addVectors([{ id: 'doc-1', vector: [...], metadata: {} }]);
const results1 = await session1.search([...], 10);
console.log('Session 1 vectors:', results1.length); // Should be 1

// Create session 2 with SAME sessionId
const session2 = await VectorDbSession.create({
  s5Portal: 'https://s5.cx',
  userSeedPhrase: 'test',
  sessionId: 'test-session-123',  // ← SAME ID
  encryptAtRest: false
});

const results2 = await session2.search([...], 10);
console.log('Session 2 vectors:', results2.length); // Should be 0 or 1?
```

**Expected**: 0 (new session)
**If you got**: 1 (reused session) ← **This is the bug!**

---

### 2. Session Identifier Parsing

Does the native layer use the **full sessionId** or parse it?

**Full sessionId from SDK**:
```
0x8D642988E3e7b6DB15b6058461d5563835b04bF6-test-db-rag-1761920951096-hjabntk2a
```

**Does native layer**:
- A) Use entire string as session key?
- B) Parse and extract parts (e.g., userAddress, dbName)?
- C) Hash the sessionId to create internal key?

**Test this**:
```javascript
// Session with unique ID 1
const session1 = await VectorDbSession.create({
  sessionId: 'user-db-rag-123-abc',
  // ...
});

// Session with unique ID 2
const session2 = await VectorDbSession.create({
  sessionId: 'user-db-rag-123-xyz',  // Same timestamp, different random
  // ...
});

// Are these treated as separate sessions?
```

---

### 3. Session Persistence Across Process

Are sessions stored in:
- A) **Memory only** (cleared when process exits)
- B) **IndexedDB** (persist across test runs)
- C) **S5 storage** (persist globally)

**Test this**:
```javascript
// Test run 1
const session = await VectorDbSession.create({ sessionId: 'persistent-test' });
await session.addVectors([...]);
// Exit process

// Test run 2 (new Node.js process)
const session2 = await VectorDbSession.create({ sessionId: 'persistent-test' });
const results = await session2.search([...], 10);
console.log('Vectors from previous run:', results.length); // Should be 0
```

**Expected**: 0 (each test run starts fresh)
**If you got**: >0 (sessions persisting in IndexedDB) ← **Possible bug!**

---

## Our Next Steps

Based on your findings, we should:

### 1. Add Session ID Uniqueness Test

```typescript
it('should isolate sessions with different session IDs', async () => {
  const session1 = await vectorManager.createSession('db1');
  await vectorManager.addVectors('db1', [
    { id: 'doc-1', values: [...], metadata: {} }
  ]);

  const session2 = await vectorManager.createSession('db2');
  await vectorManager.addVectors('db2', [
    { id: 'doc-2', values: [...], metadata: {} }
  ]);

  // Session 1 should only have doc-1
  const results1 = await vectorManager.search('db1', query, 10);
  expect(results1.length).toBe(1);
  expect(results1[0].id).toBe('doc-1');

  // Session 2 should only have doc-2
  const results2 = await vectorManager.search('db2', query, 10);
  expect(results2.length).toBe(1);
  expect(results2[0].id).toBe('doc-2');
});
```

### 2. Add Session Cleanup Verification Test

```typescript
it('should clear session data after cleanup', async () => {
  await vectorManager.createSession('test-db');
  await vectorManager.addVectors('test-db', vectors);

  // Destroy and recreate with SAME dbName
  await vectorManager.cleanup();

  // Create new VectorRAGManager (simulates next test)
  const newManager = new VectorRAGManager({ ... });
  await newManager.createSession('test-db');

  // Should have 0 vectors (new session, not reused)
  const results = await newManager.search('test-db', query, 10);
  expect(results.length).toBe(0);
});
```

### 3. Test Session ID Collision

```typescript
it('should handle rapid session creation without collision', async () => {
  // Create 100 sessions rapidly (same timestamp likely)
  const promises = [];
  for (let i = 0; i < 100; i++) {
    promises.push(vectorManager.createSession(`db-${i}`));
  }

  await Promise.all(promises);

  // Each session should be isolated
  for (let i = 0; i < 100; i++) {
    await vectorManager.addVectors(`db-${i}`, [
      { id: `doc-${i}`, values: [...], metadata: { index: i } }
    ]);
  }

  // Verify no cross-contamination
  for (let i = 0; i < 100; i++) {
    const results = await vectorManager.search(`db-${i}`, query, 10);
    expect(results.length).toBe(1);
    expect(results[0].metadata.index).toBe(i);
  }
});
```

---

## Summary

**Your hypothesis is likely correct**: The issue is in **session state management at the native layer**, not in SDK wrapper logic.

**SDK wrapper** (VectorRAGManager):
- ✅ Creates new instances for each test
- ✅ Generates unique session IDs
- ✅ Cleans up properly in afterEach
- ✅ Doesn't add default thresholds

**Native layer** (VectorDbSession):
- ❓ Might be reusing sessions when `create()` is called with existing sessionId
- ❓ Might be persisting sessions across test runs (IndexedDB?)
- ❓ Might be parsing sessionId and treating similar IDs as same session

**Next steps**:
1. Run the 3 diagnostic tests above to confirm session reuse behavior
2. Check if IndexedDB is persisting sessions across test runs
3. Verify session ID is used as full unique key, not parsed

Please let us know what you find!
