# Reproduction Guide for v0.2.1 Failures

**For**: Fabstir Vector DB Developer
**From**: SDK Developer (via Claude Code)
**Date**: October 31, 2025
**Purpose**: Provide exact reproduction steps for failing tests in SDK test suite

---

## Overview

Your local tests show 20/21 passing (95%), but our SDK tests show 19/32 passing (59%). This guide provides:

1. **Exact test code** that's failing
2. **Step-by-step reproduction** in your environment
3. **Environment details** to ensure same conditions
4. **Comparison** between your tests vs our tests

The discrepancy likely means your tests cover simpler scenarios while our SDK tests expose edge cases.

---

## Environment Setup

### SDK Test Environment

```bash
# Node version
node --version  # v22.x

# Package installed
@fabstir/vector-db-native@0.2.1 (from tarball)

# Test framework
vitest@1.6.1

# Platform
Linux 6.6.87.2-microsoft-standard-WSL2 (Docker container)
```

### How to Run Our Tests

```bash
# Clone SDK repo
git clone https://github.com/Fabstir/fabstir-llm-sdk.git
cd fabstir-llm-sdk

# Install dependencies
pnpm install

# Install v0.2.1 (if you have the tarball)
pnpm add ~/fabstir-vector-db-native-0.2.1.tgz

# Run failing tests
pnpm test tests/search/basic-search.test.ts --reporter=verbose
pnpm test tests/search/performance.test.ts --reporter=verbose
```

---

## Issue #1: topK Parameter Not Respected

### Your Test Result
✅ PASSING - Returns 3, 10, 20 as expected

### Our Test Result
❌ FAILING - Returns 1 instead of 3, 10, or 20

### Exact Test Code (Failing in SDK)

**File**: `packages/sdk-core/tests/search/basic-search.test.ts`
**Lines**: 57-82

```typescript
it('should respect topK parameter', async () => {
  const dbName = 'test-topk-limit';
  await vectorManager.createSession(dbName);

  // Create 20 vectors with random embeddings
  const vectors = Array.from({ length: 20 }, (_, i) => ({
    id: `doc-${i}`,
    values: new Array(384).fill(0).map(() => Math.random()),
    metadata: { index: i }
  }));

  await vectorManager.addVectors(dbName, vectors);

  // Test different topK values
  const query = vectors[0].values;  // Use first vector as query

  // Request 3 results
  const results3 = await vectorManager.search(dbName, query, 3);
  expect(results3.length).toBe(3);  // ❌ FAILS: receives 1 instead of 3

  // Request 10 results
  const results10 = await vectorManager.search(dbName, query, 10);
  expect(results10.length).toBe(10);  // ❌ FAILS: receives 1 instead of 10

  // Request 100 results (should cap at 20)
  const results100 = await vectorManager.search(dbName, query, 100);
  expect(results100.length).toBe(20);  // ❌ FAILS: receives 1 instead of 20
});
```

### Test Output

```
FAIL: should respect topK parameter
  Expected: 3
  Received: 1
```

### Key Differences from Your Tests

Your passing test likely differs in one of these ways:

1. **Vector count**: You might test with fewer vectors (e.g., 3-5 vs our 20)
2. **Query vector**: You might use a separate query vs our reusing `vectors[0]`
3. **Embedding generation**: You might use static embeddings vs our `Math.random()`
4. **API calls**: You might call native API directly vs our going through `VectorRAGManager`

### Reproduction Steps for Your Environment

```javascript
// In bindings/node/test/search.test.js
it('should respect topK with 20 vectors', async () => {
  const session = await VectorDbSession.create({
    s5Portal: 'https://s5.cx',
    userSeedPhrase: 'test seed phrase',
    sessionId: 'test-topk-20',
    encryptAtRest: false
  });

  // Add 20 vectors (IMPORTANT: must be 20, not less)
  const vectors = [];
  for (let i = 0; i < 20; i++) {
    const values = new Array(384).fill(0).map(() => Math.random());
    vectors.push({ id: `doc-${i}`, vector: values, metadata: { index: i } });
  }

  await session.addVectors(vectors);

  // Query with first vector's values
  const queryVector = vectors[0].vector;

  // Test k=3
  const results3 = await session.search(queryVector, 3);
  console.log('Expected: 3, Received:', results3.length);  // Should be 3
  assert.strictEqual(results3.length, 3);

  // Test k=10
  const results10 = await session.search(queryVector, 10);
  console.log('Expected: 10, Received:', results10.length);  // Should be 10
  assert.strictEqual(results10.length, 10);

  // Test k=100 (should cap at 20)
  const results100 = await session.search(queryVector, 100);
  console.log('Expected: 20, Received:', results100.length);  // Should be 20
  assert.strictEqual(results100.length, 20);
});
```

### Hypothesis

The bug might be triggered by:
- **Larger vector counts** (20+ vs smaller test sets)
- **Random embeddings** (vs static test embeddings)
- **Wrapper layer** (SDK's VectorRAGManager vs direct VectorDbSession)
- **Similarity score distribution** (all vectors might have low similarity, threshold filtering them out)

---

## Issue #3: Soft-Deleted Vectors Still in Results

### Your Test Result
⚠️ PARTIAL - 2/3 tests pass, vacuum test fails (returns 0 instead of 8-9)

### Our Test Result
❌ FAILING - Returns 0 instead of 3

### Exact Test Code (Failing in SDK)

**File**: `packages/sdk-core/tests/search/basic-search.test.ts`
**Lines**: 223-243

```typescript
it('should handle soft-deleted vectors', async () => {
  const dbName = 'test-soft-delete-search';
  await vectorManager.createSession(dbName);

  // Create 5 vectors:
  // - 2 vectors with status='delete' (indices 0, 1)
  // - 3 vectors with status='keep' (indices 2, 3, 4)
  const vectors = Array.from({ length: 5 }, (_, i) => ({
    id: `doc-${i}`,
    values: new Array(384).fill(0).map(() => Math.random()),
    metadata: { index: i, status: i < 2 ? 'delete' : 'keep' }
  }));

  await vectorManager.addVectors(dbName, vectors);

  // Soft-delete the 2 vectors with status='delete'
  await vectorManager.deleteByMetadata(dbName, { status: 'delete' });

  // Search should return only the 3 'keep' vectors
  const results = await vectorManager.search(dbName, vectors[0].values, 10);

  expect(results.length).toBe(3);  // ❌ FAILS: receives 0 instead of 3
  expect(results.every((r: any) => r.metadata.status === 'keep')).toBe(true);
});
```

### Test Output

```
FAIL: should handle soft-deleted vectors
  Expected: 3
  Received: 0
```

### Analysis

**Your observation** is correct - this looks like a **vacuum bug**, not a search bug. Here's what's happening:

1. We add 5 vectors (2 with `status='delete'`, 3 with `status='keep'`)
2. We call `deleteByMetadata({ status: 'delete' })` to soft-delete the first 2
3. **Expected**: Search returns 3 vectors (the 'keep' ones)
4. **Actual**: Search returns 0 vectors

**Two possible causes**:

#### Hypothesis A: deleteByMetadata is Too Aggressive
```javascript
// EXPECTED behavior:
deleteByMetadata({ status: 'delete' })
// Should mark 2 vectors as deleted (id='doc-0', id='doc-1')

// ACTUAL behavior (suspected):
deleteByMetadata({ status: 'delete' })
// Marking ALL 5 vectors as deleted (bug in filter logic)
```

#### Hypothesis B: Search Over-Filters
```javascript
// EXPECTED behavior:
search(query, 10)
// Should exclude soft-deleted vectors, return 3 'keep' vectors

// ACTUAL behavior (suspected):
search(query, 10)
// Excluding ALL vectors, not just soft-deleted ones
```

### Reproduction Steps for Your Environment

```javascript
// In bindings/node/test/deletion.test.js
it('should exclude soft-deleted vectors from search', async () => {
  const session = await VectorDbSession.create({
    s5Portal: 'https://s5.cx',
    userSeedPhrase: 'test seed phrase',
    sessionId: 'test-soft-delete',
    encryptAtRest: false
  });

  // Add 5 vectors with metadata
  const vectors = [];
  for (let i = 0; i < 5; i++) {
    const values = new Array(384).fill(0).map(() => Math.random());
    vectors.push({
      id: `doc-${i}`,
      vector: values,
      metadata: { index: i, status: i < 2 ? 'delete' : 'keep' }
    });
  }

  await session.addVectors(vectors);

  // BEFORE deletion: search should return 5
  const beforeDelete = await session.search(vectors[0].vector, 10);
  console.log('Before delete:', beforeDelete.length);  // Should be 5
  assert.strictEqual(beforeDelete.length, 5);

  // Soft-delete vectors with status='delete'
  const deleteResult = await session.deleteByMetadata({ status: 'delete' });
  console.log('Deleted count:', deleteResult.deletedCount);  // Should be 2
  console.log('Deleted IDs:', deleteResult.deletedIds);  // Should be ['doc-0', 'doc-1']
  assert.strictEqual(deleteResult.deletedCount, 2);

  // AFTER deletion: search should return 3 (only 'keep' vectors)
  const afterDelete = await session.search(vectors[0].vector, 10);
  console.log('After delete:', afterDelete.length);  // Should be 3
  console.log('Statuses:', afterDelete.map(r => r.metadata.status));  // Should be all 'keep'

  assert.strictEqual(afterDelete.length, 3);  // ❌ YOUR VACUUM TEST FAILS HERE (gets 0)
  assert.ok(afterDelete.every(r => r.metadata.status === 'keep'));
});
```

### Debugging Steps

To narrow down where the bug is:

```javascript
// Step 1: Check deleteByMetadata result
const deleteResult = await session.deleteByMetadata({ status: 'delete' });
console.log('Deleted count:', deleteResult.deletedCount);
console.log('Deleted IDs:', deleteResult.deletedIds);
// EXPECTED: { deletedCount: 2, deletedIds: ['doc-0', 'doc-1'] }
// If deletedCount !== 2, the filter logic in deleteByMetadata is broken

// Step 2: Check search without vacuum
const searchResults = await session.search(queryVector, 10);
console.log('Search results count:', searchResults.length);
console.log('Search result IDs:', searchResults.map(r => r.id));
// EXPECTED: 3 results with IDs ['doc-2', 'doc-3', 'doc-4']
// If 0 results, search is over-filtering

// Step 3: Check stats after deletion
const stats = await session.getStats();
console.log('Total vectors:', stats.totalVectors);
console.log('Deleted vectors:', stats.deletedVectors);  // If this field exists
// EXPECTED: totalVectors=5, deletedVectors=2 (or similar)
```

---

## Issue #4: $gt/$lt Operators

### Your Test Result
✅ PASSING - All 8 operator tests pass

### Our Test Result
⚠️ **NOT VERIFIED** - Tests still skipped

### Why We Didn't Verify

The tests remain marked with `.skip()` in our test suite because v0.2.0 didn't support these operators. Since you claim they're now supported in v0.2.1, we should unskip these tests.

**File**: `packages/sdk-core/tests/search/filtering.test.ts`
**Lines**: 103-133 (skipped test)

```typescript
it.skip('should filter with $gt and $lt operators (NOT SUPPORTED in v0.2.0)', async () => {
  // ... test code ...
});
```

### Action Needed

We'll unskip these tests and verify if $gt/$lt actually work. If your 8 operator tests pass, ours should too.

---

## Comparison: Your Tests vs Our Tests

### What Your Tests Likely Do (Simpler)

```javascript
// Typical passing test structure
it('should respect topK', async () => {
  const vectors = [
    { id: '1', vector: [1, 2, 3, ...], metadata: {} },
    { id: '2', vector: [4, 5, 6, ...], metadata: {} },
    { id: '3', vector: [7, 8, 9, ...], metadata: {} }
  ];

  await session.addVectors(vectors);
  const results = await session.search([1, 2, 3, ...], 2);

  assert.strictEqual(results.length, 2);  // ✅ PASSES
});
```

**Why it passes**:
- Small vector count (3)
- Static, simple embeddings
- Query matches first vector exactly
- High similarity scores

### What Our Tests Do (More Complex)

```typescript
it('should respect topK parameter', async () => {
  // Generate 20 vectors with RANDOM embeddings
  const vectors = Array.from({ length: 20 }, (_, i) => ({
    id: `doc-${i}`,
    values: new Array(384).fill(0).map(() => Math.random()),  // Random!
    metadata: { index: i }
  }));

  await vectorManager.addVectors(dbName, vectors);

  // Query with first vector (but embeddings are random)
  const results = await vectorManager.search(dbName, vectors[0].values, 3);

  expect(results.length).toBe(3);  // ❌ FAILS: gets 1
});
```

**Why it fails**:
- Larger vector count (20)
- Random embeddings (harder to predict similarity)
- Goes through SDK wrapper (VectorRAGManager)
- Similarity scores might vary widely

### The Edge Case

The bug might be:

```rust
// Suspected buggy logic (pseudocode)
fn search(query: Vec<f32>, k: usize) -> Vec<SearchResult> {
    let candidates = find_approximate_neighbors(query, k * 10);

    let scored = candidates.iter()
        .map(|c| (c, similarity_score(query, c)))
        .collect();

    // BUG: If similarity scores are all < some threshold,
    // this might return empty or only 1 result
    let filtered = scored.iter()
        .filter(|(_, score)| *score > 0.9)  // Hardcoded threshold?
        .take(k)
        .collect();

    filtered  // Returns 1 instead of k
}
```

**In simple tests**: Static embeddings → high similarity → passes filter
**In complex tests**: Random embeddings → low similarity → fails filter

---

## How to Access Our Full Test Suite

### Option 1: Clone SDK Repo (Recommended)

```bash
git clone https://github.com/Fabstir/fabstir-llm-sdk.git
cd fabstir-llm-sdk

# Install v0.2.1
pnpm install
pnpm add ~/fabstir-vector-db-native-0.2.1.tgz

# Run failing tests
pnpm test tests/search/basic-search.test.ts
pnpm test tests/search/performance.test.ts
```

### Option 2: Copy Test Files

We can provide the exact test files:
- `packages/sdk-core/tests/search/basic-search.test.ts` (289 lines)
- `packages/sdk-core/tests/search/filtering.test.ts` (335 lines)
- `packages/sdk-core/tests/search/performance.test.ts` (299 lines)

### Option 3: Video Call / Screen Share

If easier, we can do a screen share to show the failures in real-time and debug together.

---

## Summary for Developer

**What We Know**:
1. Your tests: 20/21 passing (95%) - simpler scenarios
2. Our tests: 19/32 passing (59%) - more complex scenarios
3. Bugs are **context-dependent** - work in simple cases, fail in complex ones

**What We Need from You**:
1. Try running our exact test code in your environment
2. Test with **20 vectors** (not 3-5) using **random embeddings**
3. Debug `deleteByMetadata` to see if it's marking ALL vectors as deleted
4. Check if search has a hidden similarity threshold that's filtering out results

**What We Can Provide**:
- Full access to test suite (GitHub)
- Step-by-step reproduction in your test framework
- Video call to debug together
- More test scenarios if needed

---

## Next Steps

Please try running the exact reproduction steps above and let us know:

1. **topK test**: Does your test with 20 random vectors return 1 or 3?
2. **Soft-delete test**: What does `deleteByMetadata` return? (deletedCount, deletedIds)
3. **After deleteByMetadata**: What does search return? (0 or 3?)

Once we can reproduce the same failures in your environment, we can debug together and find the root cause.

---

**Contact**: Respond via SDK developer or create GitHub issue with reproduction results

**Test Suite Location**: https://github.com/Fabstir/fabstir-llm-sdk/tree/feature/rag-integration/packages/sdk-core/tests/search

**Verification Report**: `docs/fabstir-vectordb-reference/VERIFICATION_REPORT_V0.2.1.md` (comprehensive analysis)
