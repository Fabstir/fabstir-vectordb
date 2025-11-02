# topK Bug Fix - v0.2.2 Complete

**Date**: October 31, 2025
**Status**: ✅ **ALL BUGS FIXED**
**Version**: 0.2.2
**Test Results**: 21/21 passing (100%)

---

## Executive Summary

The topK bug has been **identified and fixed**. Your diagnostic tests were instrumental in finding the root cause.

**Root Cause**: Hidden default similarity threshold of `0.7` was filtering results BEFORE topK limit was applied.

**Fix**: Changed default threshold from `0.7` → `0.0` in `bindings/node/src/session.rs:227`

**Impact**: All 5 original bugs from v0.2.0 are now FIXED ✅

---

## The Bug: Hidden Default Threshold

### Location
**File**: `bindings/node/src/session.rs`
**Line**: 227

### Before (Broken)
```rust
let threshold = options.as_ref()
    .and_then(|o| o.threshold)
    .unwrap_or(0.7) as f32; // ← Hidden default of 0.7!
```

### After (Fixed)
```rust
let threshold = options.as_ref()
    .and_then(|o| o.threshold)
    .unwrap_or(0.0) as f32; // ← Default to 0.0 (no filtering)
```

### How It Worked

1. **User calls** `session.search(query, 10)` (no threshold specified)
2. **Native layer applies** default threshold of 0.7
3. **Filters results** where `similarity >= 0.7`
4. **Random embeddings** produce similarities ~0.11, only exact match (1.0) passes filter
5. **Returns 1 result** instead of 10

### Why This Affected Random Embeddings

Random 384-dimensional vectors have very low cosine similarity:
- Exact match: `score = 1.0` ✅ (passes threshold)
- Random vectors: `score ≈ 0.11` ❌ (filtered out by threshold 0.7)

Result: Only the exact match vector passes the threshold filter, so topK returns 1 instead of K.

---

## Diagnostic Test Results

### Test 1: Explicit threshold=0
```javascript
const results = await session.search(query, 10, { threshold: 0 });
```
- **Before fix**: 10 results ✅ (explicitly bypassed default)
- **After fix**: 10 results ✅ (same behavior)
- **Conclusion**: Explicit threshold always worked

### Test 2: Query NOT in dataset
```javascript
const newQuery = new Array(384).fill(0).map(() => Math.random());
const results = await session.search(newQuery, 10, { threshold: 0 });
```
- **Before fix**: 10 results ✅ (threshold=0 bypassed default)
- **After fix**: 10 results ✅
- **Conclusion**: Low similarity scores work when threshold is explicit

### Test 3: No options (default behavior)
```javascript
const resultsNoOptions = await session.search(query, 10);
const resultsWithThreshold = await session.search(query, 10, { threshold: 0 });
```
- **Before fix**:
  - No options: 1 result ❌
  - With threshold=0: 10 results ✅
  - **Implied default threshold: ~0.12**
- **After fix**:
  - No options: 10 results ✅
  - With threshold=0: 10 results ✅
  - **No default threshold**

### Test 4: Exact SDK scenario
```javascript
const results3 = await session.search(query, 3);
const results10 = await session.search(query, 10);
```
- **Before fix**:
  - k=3: Got 1 ❌
  - k=10: Got 1 ❌
- **After fix**:
  - k=3: Got 3 ✅
  - k=10: Got 10 ✅

---

## Complete Test Results - v0.2.2

### All Bug Fix Tests: 21/21 Passing ✅

#### Issue #1: topK Parameter (1/1 passing)
```
✅ should return k results when k < total vectors
   - k=3 returns 3 results
   - k=10 returns 10 results
   - k=100 returns 20 results (capped at dataset size)
```

#### Issue #2: includeVectors Option (4/4 passing)
```
✅ should NOT include vectors by default
✅ should include vectors when includeVectors = true
✅ should NOT include vectors when includeVectors = false
✅ should return correct vector values
```

#### Issue #3: Soft-Deleted Vectors (3/3 passing)
```
✅ deleted vector should not appear in search results
✅ deleteByMetadata should remove matching vectors from search
✅ vacuum should physically remove deleted vectors
```

#### Issue #4: $gt/$lt Operators (8/8 passing)
```
✅ $gt: strictly greater than (excludes boundary)
✅ $gte: greater than or equal (includes boundary)
✅ $lt: strictly less than (excludes boundary)
✅ $lte: less than or equal (includes boundary)
✅ $gt and $lt combined (exclusive range)
✅ $gte and $lte combined (inclusive range)
✅ mixed inclusive/exclusive range: $gte and $lt
✅ mixed inclusive/exclusive range: $gt and $lte
```

#### Issue #5: Dimension Validation (5/5 passing)
```
✅ should throw error when query dimension is too large
✅ should throw error when query dimension is too small
✅ should succeed when query dimension matches index dimension
✅ should provide clear error message format
✅ should work correctly with filters even when checking dimensions
```

---

## What Changed Between v0.2.1 → v0.2.2

### Code Changes
1. **File**: `bindings/node/src/session.rs`
   - **Line 227**: Changed `unwrap_or(0.7)` → `unwrap_or(0.0)`
   - **Comment updated**: Added explanation about default threshold behavior

### Test Results Improvement
- **v0.2.1**: 19/32 SDK tests passing (59%) - topK broken
- **v0.2.2**: 21/21 native tests passing (100%) - all bugs fixed

---

## Migration Guide for SDK

### No Breaking Changes
The fix is **backward compatible**. Existing code continues to work:

#### If You Were Working Around the Bug
```javascript
// Before (workaround for broken default threshold)
const results = await session.search(query, 10, { threshold: 0 });

// After fix (still works, threshold=0 is explicit)
const results = await session.search(query, 10, { threshold: 0 });
```

#### If You Want Default Behavior
```javascript
// Before (broken - returned 1 result with random embeddings)
const results = await session.search(query, 10);

// After fix (works correctly - returns 10 results)
const results = await session.search(query, 10);
```

#### If You Want a Custom Threshold
```javascript
// Works in both versions
const results = await session.search(query, 10, { threshold: 0.5 });
// Returns only vectors with similarity >= 0.5
```

### Recommended Testing

Run your full SDK test suite with v0.2.2 and verify:

1. **topK tests now pass** (should go from 7/11 → 11/11 in basic-search.test.ts)
2. **Performance tests pass** (should go from 2/7 → 7/7 in performance.test.ts)
3. **Overall**: Expect 30/32 tests passing (94%)

Remaining failures should only be:
- Skipped tests (caching, history tracking - deferred features)

---

## Installation

### Update to v0.2.2
```bash
# Option 1: Install from tarball
npm install /path/to/fabstir-vector-db-native-0.2.2.tgz

# Option 2: If published to npm
npm install @fabstir/vector-db-native@0.2.2
```

### Verify Installation
```bash
npm list @fabstir/vector-db-native
# Should show: @fabstir/vector-db-native@0.2.2
```

---

## Technical Details

### Similarity Score Calculation

The native layer converts Euclidean distance to similarity score:

```rust
let score = 1.0 / (1.0 + distance);
```

**Score ranges**:
- Exact match: `distance=0` → `score=1.0`
- Near match: `distance=0.5` → `score=0.67`
- Random vectors: `distance≈9` → `score≈0.1`

### Threshold Filtering Logic

```rust
let search_results: Vec<SearchResult> = results
    .into_iter()
    .filter(|r| {
        let score = 1.0 / (1.0 + r.distance);
        score >= threshold  // ← Threshold filter applied HERE
    })
    .map(|r| { /* ... */ })
    .collect();
```

**Before fix**: `threshold=0.7` by default → filters out random vectors
**After fix**: `threshold=0.0` by default → no filtering (pure topK)

---

## Why Your Cleanup Fix Didn't Solve topK

Your cleanup fix (calling `session.destroy()` properly) **was necessary and correct**:
- ✅ Fixed session contamination between tests
- ✅ Eliminated warnings about undestroyed sessions
- ✅ Improved test isolation

But the topK bug **persisted after cleanup** because:
- The hidden threshold was applied **inside each search call**
- Not related to session state or contamination
- Would affect even the first test with a fresh session

Both fixes were needed:
1. **Cleanup fix** (in SDK wrapper) - prevents session leaks
2. **Threshold fix** (in native layer) - fixes topK behavior

---

## Acknowledgments

Thank you for:
1. Providing detailed reproduction guides with exact test code
2. Creating diagnostic tests that pinpointed the threshold issue
3. Fixing the session cleanup bug in the SDK wrapper
4. Your patience and excellent debugging methodology

The combination of:
- Your SDK-level cleanup fix
- Our native-level threshold fix

Has resulted in a **fully functional v0.2.2 release** with all 5 bugs resolved.

---

## Files Included

- **Tarball**: `fabstir-vector-db-native-0.2.2.tgz` (3.0 MB)
- **Test files** (in `bindings/node/test/`):
  - `test-topk-diagnostics.js` (4 diagnostic tests that found the bug)
  - `test-topk-bug.js` (1 test)
  - `test-include-vectors.js` (4 tests)
  - `test-soft-deletion.js` (3 tests)
  - `test-gt-lt-operators.js` (8 tests)
  - `test-dimension-validation.js` (5 tests)

---

## Next Steps

1. **Install v0.2.2**: Use the provided tarball
2. **Run SDK tests**: Verify 30/32 passing (94%)
3. **Report results**: Let us know if any unexpected failures remain
4. **Production deployment**: v0.2.2 is ready for production use

---

**Version**: 0.2.2
**Release Date**: October 31, 2025
**Status**: Production Ready ✅
**Test Coverage**: 21/21 tests passing (100%)
