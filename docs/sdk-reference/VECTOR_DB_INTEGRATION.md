# Vector DB Integration Guide for fabstir-llm-sdk

**Target Audience:** SDK Developers
**Last Updated:** 2025-10-26
**Status:** Implementation Ready

## Table of Contents

- [Architecture Overview](#architecture-overview)
- [Why Native Node.js Bindings](#why-native-nodejs-bindings)
- [Package Installation](#package-installation)
- [API Reference](#api-reference)
- [Integration Guide](#integration-guide)
- [RAG Flow Examples](#rag-flow-examples)
- [Testing Strategy](#testing-strategy)
- [Security Considerations](#security-considerations)
- [Performance Characteristics](#performance-characteristics)
- [Troubleshooting](#troubleshooting)

---

## Architecture Overview

### The P2P Model

Platformless AI operates as a **decentralized P2P network** where:

```
User (owns data via blockchain seed phrase)
  ↓ [Encrypted XChaCha20-Poly1305]
Host (stateless Docker container with GPU)
  ↓ [fabstir-llm-sdk with embedded vector-db-native]
S5 Storage (user's vectors under user's CID)
  ↓ [Sia Proof of Storage]
Sia Network (verifiable decentralized storage)
```

**Key Principles:**

1. **Data Sovereignty:** Users own all data via blockchain-derived seed phrases
2. **Stateless Hosts:** Hosts are ephemeral compute units (like AWS Lambda, but P2P)
3. **No Centralization:** No central vector DB server that would re-centralize the architecture
4. **Privacy-First:** All communication encrypted, no cross-user data leakage
5. **Verifiability:** 4-layer verification stack (CID, Proof of Storage, STARK proofs, encryption)

### Session Lifecycle

```typescript
// 1. User initiates session with host
const { sessionId } = await sessionManager.startSession(model, host, config);

// 2. Host loads user's vectors from S5 (if RAG enabled)
// Vector DB session created in-memory

// 3. Multi-turn conversation with RAG context
const response1 = await sessionManager.sendPrompt(sessionId, "query 1");
// → Retrieves relevant context from user's vectors
// → Augments prompt with context
// → Sends to LLM

const response2 = await sessionManager.sendPrompt(sessionId, "query 2");
// → Same process, uses session-cached vectors

// 4. Session ends - host clears all memory
await sessionManager.endSession(sessionId);
// → Vector DB session destroyed
// → Host is stateless again
```

### Progressive RAG Approach

Platformless AI implements a **progressive enhancement** strategy:

1. **MVP (Current):** Simple context passing, no vector DB needed
2. **Phase 2 (Compaction):** Embedding generation for context compaction
3. **Phase 3 (Full RAG):** Vector search for retrieval-augmented generation

This integration enables **Phase 3** while maintaining decentralization.

---

## Why Native Node.js Bindings

### The Centralization Problem

Initial proposals considered:

- ❌ Separate vector DB server → Re-centralizes P2P architecture
- ❌ Container orchestration → Too complex for stateless hosts
- ❌ Process pool isolation → Unnecessary overhead

### The Solution: Embedded Library

Native Node.js bindings provide:

- ✅ **Fully Decentralized:** No central server, each host independent
- ✅ **Stateless Hosts:** Library loads/clears with session lifecycle
- ✅ **High Performance:** Native Rust code for vector operations
- ✅ **Standard Distribution:** npm package (tarball or registry)
- ✅ **Full Feature Set:** Hybrid HNSW/IVF indexing, S5 integration
- ✅ **Simple Integration:** Works like any npm dependency

### Architecture Comparison

**❌ Server-Based (Centralized):**

```
User → Host → [Central Vector DB Server] → S5
         ↑
    Creates centralization point!
```

**✅ Embedded Library (Decentralized):**

```
User → Host (with embedded vector-db-native) → S5
         ↑
    Each host independent, fully P2P!
```

---

## Package Installation

### Prerequisites

The vector DB native module requires:

- Node.js >= 16.0.0
- Linux x64 (primary platform for hosts)
- Optional: ARM64 for ARM-based hosts

### Install from Tarball

The fabstir-vectordb team provides prebuilt binaries via tarball:

```bash
# Option 1: Local tarball
npm install /path/to/fabstir-vector-db-native-0.1.0.tgz

# Option 2: From URL
npm install https://github.com/yourorg/fabstir-vectordb/releases/download/v0.1.0/fabstir-vector-db-native-0.1.0.tgz

# Option 3: Git dependency (bindings/node subdirectory)
npm install git+https://github.com/yourorg/fabstir-vectordb.git#main:bindings/node
```

### Add to fabstir-llm-sdk

```json
// package.json
{
  "name": "fabstir-llm-sdk",
  "dependencies": {
    "@fabstir/vector-db-native": "file:../fabstir-vectordb/bindings/node"
    // ... other dependencies
  }
}
```

### Verify Installation

```typescript
import { VectorDBSession } from "@fabstir/vector-db-native";

console.log("Vector DB native module loaded successfully!");
```

---

## API Reference

### VectorDBSession

The main class for managing user vector sessions.

#### Static Methods

##### `VectorDBSession.create(config)`

Creates a new vector DB session for a user.

**Parameters:**

```typescript
interface VectorDBConfig {
  s5Portal: string; // S5 portal URL (e.g., 'http://localhost:5524')
  userSeedPhrase: string; // User's blockchain-derived seed phrase
  sessionId: string; // Unique session identifier
  memoryBudgetMB?: number; // Optional: Memory limit (default: 512 MB)
}
```

**Returns:** `Promise<VectorDBSession>`

**Example:**

```typescript
const session = await VectorDBSession.create({
  s5Portal: process.env.S5_PORTAL_URL || "http://localhost:5524",
  userSeedPhrase: userSeed,
  sessionId: sessionId.toString(),
  memoryBudgetMB: 512,
});
```

**Throws:**

- `VectorDBError` if S5 connection fails
- `VectorDBError` if invalid configuration

---

#### Instance Methods

##### `session.loadUserVectors(cid, options?)`

Loads user's vector index from S5 storage.

**Parameters:**

```typescript
interface LoadOptions {
  lazyLoad?: boolean; // Load HNSW immediately, IVF on-demand (default: true)
  memoryBudgetMB?: number; // Override session memory budget
}
```

**Returns:** `Promise<void>`

**Example:**

```typescript
// Lazy load (recommended for large datasets)
await session.loadUserVectors(userVectorCID, {
  lazyLoad: true,
  memoryBudgetMB: 512,
});

// Full load (for smaller datasets or when latency is critical)
await session.loadUserVectors(userVectorCID, {
  lazyLoad: false,
});
```

**Load Strategy:**

- **Lazy Mode (default):** Loads HNSW index + IVF centroids immediately, loads IVF clusters on-demand during search
- **Full Mode:** Loads entire index into memory upfront

**Performance:**

- Lazy mode: 2-5s initial load for 1M vectors
- Full mode: 10-30s initial load for 1M vectors
- Memory usage: ~200MB per 100K vectors (384-dim)

---

##### `session.search(queryVector, k, options?)`

Searches for similar vectors using hybrid HNSW/IVF indexing.

**Parameters:**

```typescript
interface SearchOptions {
  threshold?: number; // Minimum similarity score (0-1, default: 0.7)
  filters?: Record<string, any>; // Metadata filters (future enhancement)
  includeVectors?: boolean; // Return vectors in results (default: false)
}
```

**Returns:**

```typescript
Promise<
  Array<{
    id: string; // Vector ID
    score: number; // Similarity score (0-1, higher is more similar)
    metadata: any; // Associated metadata
    vector?: number[]; // Original vector (if includeVectors: true)
  }>
>;
```

**Example:**

```typescript
const results = await session.search(queryEmbedding, 5, {
  threshold: 0.7,
  includeVectors: false,
});

for (const result of results) {
  console.log(`ID: ${result.id}, Score: ${result.score}`);
  console.log(`Text: ${result.metadata.text}`);
}
```

**Performance:**

- Latency: < 50ms for 1M vectors (p99)
- Automatically routes between HNSW (recent) and IVF (historical) indices
- Results sorted by similarity score (descending)

---

##### `session.addVectors(vectors)`

Adds new vectors to the session index (for compaction feature).

**Parameters:**

```typescript
interface VectorInput {
  id: string; // Unique identifier
  vector: number[]; // Dense embedding vector
  metadata: any; // Associated metadata (must be JSON-serializable)
}
```

**Returns:** `Promise<void>`

**Example:**

```typescript
await session.addVectors([
  {
    id: 'doc1_chunk1',
    vector: [0.1, 0.2, ..., 0.5], // 384-dim for all-MiniLM-L6-v2
    metadata: {
      text: 'This is the content...',
      documentId: 'doc1',
      chunkIndex: 0,
      timestamp: Date.now()
    }
  },
  // ... more vectors
]);
```

**Notes:**

- Vectors are added to the in-memory index
- Call `saveToS5()` to persist changes
- All vectors must have same dimensionality

---

##### `session.saveToS5()`

Saves the current index state to S5 storage.

**Returns:** `Promise<string>` - Returns new CID

**Example:**

```typescript
const newCID = await session.saveToS5();
console.log("Vectors saved to S5:", newCID);

// Store CID for future sessions
await storageManager.updateUserSettings({
  vectorIndexCID: newCID,
});
```

**Performance:**

- Serializes index to CBOR format
- Uploads to S5 with retry logic
- Returns content-addressed CID
- Time: ~5-20s depending on index size

---

##### `session.destroy()`

**CRITICAL:** Clears all session data from memory.

**Returns:** `Promise<void>`

**Example:**

```typescript
try {
  // ... use session
} finally {
  await session.destroy(); // ALWAYS call in finally block
}
```

**Security:**

- Clears all user vectors from host memory
- Releases all allocated resources
- MUST be called when session ends
- Host becomes stateless again

---

##### `session.getStats()`

Returns session statistics for monitoring.

**Returns:**

```typescript
interface SessionStats {
  vectorCount: number; // Total vectors in index
  memoryUsageMB: number; // Current memory usage
  indexType: "hnsw" | "ivf" | "hybrid"; // Active index type
  hnswVectorCount?: number; // Vectors in HNSW index
  ivfVectorCount?: number; // Vectors in IVF index
}
```

**Example:**

```typescript
const stats = session.getStats();
console.log(`Loaded ${stats.vectorCount} vectors`);
console.log(`Memory usage: ${stats.memoryUsageMB} MB`);
console.log(`Index type: ${stats.indexType}`);
```

---

## Integration Guide

### Step 1: Create RAG Manager

Create a new module to manage vector DB sessions:

```typescript
// src/rag/VectorRAGManager.ts
import { VectorDBSession } from "@fabstir/vector-db-native";
import type { S5Config } from "../storage/StorageManager";

export interface RAGConfig {
  s5Config: S5Config;
  memoryBudgetMB?: number;
  lazyLoad?: boolean;
}

export class VectorRAGManager {
  private session?: VectorDBSession;
  private config: RAGConfig;

  constructor(config: RAGConfig) {
    this.config = config;
  }

  /**
   * Initialize RAG session for a user
   */
  async initializeSession(
    sessionId: string,
    userSeedPhrase: string,
    userVectorCID?: string
  ): Promise<void> {
    // Create session
    this.session = await VectorDBSession.create({
      s5Portal: this.config.s5Config.portalUrl || "http://localhost:5524",
      userSeedPhrase,
      sessionId,
      memoryBudgetMB: this.config.memoryBudgetMB || 512,
    });

    // Load user's vectors if they exist
    if (userVectorCID) {
      await this.session.loadUserVectors(userVectorCID, {
        lazyLoad: this.config.lazyLoad ?? true,
      });

      const stats = this.session.getStats();
      console.log(
        `RAG initialized: ${stats.vectorCount} vectors, ${stats.memoryUsageMB} MB`
      );
    } else {
      console.log("RAG initialized: No existing vectors for this user");
    }
  }

  /**
   * Retrieve relevant context for a query
   */
  async retrieveContext(
    queryEmbedding: number[],
    k: number = 5,
    threshold: number = 0.7
  ): Promise<Array<{ text: string; score: number; metadata: any }>> {
    if (!this.session) {
      throw new Error("RAG session not initialized");
    }

    const results = await this.session.search(queryEmbedding, k, {
      threshold,
      includeVectors: false,
    });

    return results.map((r) => ({
      text: r.metadata.text || "",
      score: r.score,
      metadata: r.metadata,
    }));
  }

  /**
   * Add new vectors to the index (for compaction)
   */
  async addVectors(
    vectors: Array<{
      id: string;
      vector: number[];
      metadata: any;
    }>
  ): Promise<void> {
    if (!this.session) {
      throw new Error("RAG session not initialized");
    }

    await this.session.addVectors(vectors);
  }

  /**
   * Save updated index to S5
   */
  async saveIndex(): Promise<string> {
    if (!this.session) {
      throw new Error("RAG session not initialized");
    }

    return await this.session.saveToS5();
  }

  /**
   * CRITICAL: Cleanup session (call on session end)
   */
  async cleanup(): Promise<void> {
    if (this.session) {
      await this.session.destroy();
      this.session = undefined;
    }
  }

  /**
   * Get session statistics
   */
  getStats(): any {
    return this.session?.getStats() || null;
  }
}
```

---

### Step 2: Extend SessionManager

Integrate RAG into the existing SessionManager:

```typescript
// src/session/SessionManager.ts
import { VectorRAGManager } from "../rag/VectorRAGManager";

export class SessionManager {
  private ragManager?: VectorRAGManager;

  /**
   * Start a new LLM session with optional RAG
   */
  async startSession(
    model: string,
    provider: string,
    config: SessionConfig & { enableRAG?: boolean }
  ): Promise<{ sessionId: bigint; jobId: bigint }> {
    // Create blockchain session
    const { sessionId, jobId } = await super.startSession(
      model,
      provider,
      config
    );

    // Initialize RAG if enabled and user has vectors
    if (config.enableRAG !== false) {
      // Default to true
      try {
        // Get user's vector CID from storage
        const storageManager = await this.sdk.getStorageManager();
        const userSettings = await storageManager.getUserSettings();
        const userVectorCID = userSettings?.vectorIndexCID;

        // Initialize RAG manager
        this.ragManager = new VectorRAGManager({
          s5Config: this.sdk.config.s5Config || {},
          memoryBudgetMB: config.ragMemoryBudgetMB || 512,
          lazyLoad: config.ragLazyLoad ?? true,
        });

        // Initialize session
        await this.ragManager.initializeSession(
          sessionId.toString(),
          this.sdk.userSeedPhrase, // User's blockchain-derived seed
          userVectorCID
        );

        console.log("RAG enabled for session", sessionId.toString());
      } catch (error) {
        console.warn(
          "RAG initialization failed, continuing without RAG:",
          error
        );
        this.ragManager = undefined;
      }
    }

    return { sessionId, jobId };
  }

  /**
   * Send prompt with RAG context
   */
  async sendPrompt(
    sessionId: bigint,
    prompt: string,
    options?: {
      ragEnabled?: boolean;
      ragK?: number;
      ragThreshold?: number;
    }
  ): Promise<string> {
    let finalPrompt = prompt;

    // Retrieve RAG context if enabled
    if (this.ragManager && options?.ragEnabled !== false) {
      try {
        // 1. Generate embedding for the query
        const queryEmbedding = await this.generateEmbedding(prompt);

        // 2. Retrieve relevant context
        const contexts = await this.ragManager.retrieveContext(
          queryEmbedding,
          options?.ragK || 5,
          options?.ragThreshold || 0.7
        );

        // 3. Augment prompt with context
        if (contexts.length > 0) {
          const contextText = contexts
            .map(
              (c, i) =>
                `[${i + 1}] ${c.text} (relevance: ${c.score.toFixed(2)})`
            )
            .join("\n\n");

          finalPrompt = `Context from your knowledge base:\n${contextText}\n\n---\n\nUser Query: ${prompt}`;

          console.log(`RAG: Retrieved ${contexts.length} relevant contexts`);
        }
      } catch (error) {
        console.warn(
          "RAG retrieval failed, continuing without context:",
          error
        );
      }
    }

    // Send (possibly augmented) prompt to LLM
    return await super.sendPrompt(sessionId, finalPrompt);
  }

  /**
   * End session and cleanup RAG
   */
  async endSession(sessionId: bigint): Promise<void> {
    // CRITICAL: Cleanup RAG session
    if (this.ragManager) {
      try {
        await this.ragManager.cleanup();
        console.log("RAG session cleaned up");
      } catch (error) {
        console.error("RAG cleanup error:", error);
      } finally {
        this.ragManager = undefined;
      }
    }

    // End blockchain session
    await super.endSession(sessionId);
  }

  /**
   * Helper: Generate embedding for text
   * NOTE: This should use the host's embedding model
   */
  private async generateEmbedding(text: string): Promise<number[]> {
    // TODO: Integrate with host's embedding model
    // For now, throw error - SDK developer needs to implement this
    throw new Error(
      "Embedding generation not implemented - integrate with host embedding model"
    );
  }
}
```

---

### Step 3: Add Embedding Generation

Integrate with the host's embedding model:

```typescript
// src/embeddings/EmbeddingService.ts
export class EmbeddingService {
  private modelEndpoint: string;

  constructor(endpoint: string = "http://localhost:8081/embed") {
    this.modelEndpoint = endpoint;
  }

  /**
   * Generate embedding for text using host's model
   */
  async generateEmbedding(text: string): Promise<number[]> {
    const response = await fetch(this.modelEndpoint, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ text }),
    });

    if (!response.ok) {
      throw new Error(`Embedding generation failed: ${response.statusText}`);
    }

    const data = await response.json();
    return data.embedding;
  }

  /**
   * Batch generate embeddings
   */
  async generateEmbeddings(texts: string[]): Promise<number[][]> {
    const response = await fetch(this.modelEndpoint + "/batch", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ texts }),
    });

    if (!response.ok) {
      throw new Error(
        `Batch embedding generation failed: ${response.statusText}`
      );
    }

    const data = await response.json();
    return data.embeddings;
  }
}
```

Then integrate into SessionManager:

```typescript
import { EmbeddingService } from "../embeddings/EmbeddingService";

export class SessionManager {
  private embeddingService: EmbeddingService;

  constructor(sdk: FabstirSDKCore) {
    super(sdk);
    this.embeddingService = new EmbeddingService(
      process.env.EMBEDDING_ENDPOINT || "http://localhost:8081/embed"
    );
  }

  private async generateEmbedding(text: string): Promise<number[]> {
    return await this.embeddingService.generateEmbedding(text);
  }
}
```

---

## RAG Flow Examples

### Example 1: Basic RAG Session

```typescript
import { FabstirSDKCore } from "@fabstir/sdk-core";

// Initialize SDK
const sdk = new FabstirSDKCore({
  rpcUrl: process.env.RPC_URL,
  contractAddresses: {
    /* ... */
  },
});

await sdk.authenticate("signer", { signer: userSigner });

const sessionManager = sdk.getSessionManager();

// Start session with RAG enabled (default)
const { sessionId } = await sessionManager.startSession(
  modelHash,
  hostAddress,
  {
    depositAmount: "1.0",
    pricePerToken: 200,
    duration: 3600,
    proofInterval: 100,
    enableRAG: true, // Explicitly enable (default behavior)
    ragMemoryBudgetMB: 512,
    ragLazyLoad: true,
  }
);

// Send prompts - RAG automatically retrieves context
const response1 = await sessionManager.sendPrompt(
  sessionId,
  "What did I say about climate change?"
);
// → Searches user's vectors for "climate change" context
// → Augments prompt with relevant past conversations/documents
// → Sends to LLM

console.log(response1);

// Multi-turn conversation with persistent context
const response2 = await sessionManager.sendPrompt(
  sessionId,
  "Can you summarize the key points?"
);
// → Uses same loaded vectors (no reload needed)

// End session - cleanup happens automatically
await sessionManager.endSession(sessionId);
// → Vector DB session destroyed
// → Host memory cleared
```

---

### Example 2: Document Ingestion with Compaction

```typescript
import { VectorRAGManager } from "fabstir-llm-sdk/rag";

// User uploads a document
const documentText = "...long document content...";

// Split into chunks
const chunks = splitIntoChunks(documentText, {
  chunkSize: 500,
  overlap: 50,
});

// Generate embeddings for each chunk
const embeddingService = new EmbeddingService();
const embeddings = await Promise.all(
  chunks.map((chunk) => embeddingService.generateEmbedding(chunk))
);

// Create RAG manager
const ragManager = new VectorRAGManager({
  s5Config: sdk.config.s5Config,
});

// Initialize with user's existing vectors (if any)
await ragManager.initializeSession(
  sessionId.toString(),
  userSeedPhrase,
  existingVectorCID
);

// Add new vectors
await ragManager.addVectors(
  chunks.map((chunk, i) => ({
    id: `doc_${documentId}_chunk_${i}`,
    vector: embeddings[i],
    metadata: {
      text: chunk,
      documentId,
      chunkIndex: i,
      timestamp: Date.now(),
    },
  }))
);

// Save to S5
const newCID = await ragManager.saveIndex();
console.log("Updated vectors saved:", newCID);

// Update user settings with new CID
const storageManager = await sdk.getStorageManager();
await storageManager.updateUserSettings({
  vectorIndexCID: newCID,
});

// Cleanup
await ragManager.cleanup();
```

---

### Example 3: Custom RAG Parameters

```typescript
// Start session with custom RAG settings
const { sessionId } = await sessionManager.startSession(
  modelHash,
  hostAddress,
  {
    depositAmount: "1.0",
    pricePerToken: 200,
    duration: 3600,
    proofInterval: 100,
    ragMemoryBudgetMB: 1024, // Increase memory budget
    ragLazyLoad: false, // Full load for low latency
  }
);

// Send prompt with custom RAG parameters
const response = await sessionManager.sendPrompt(
  sessionId,
  "What are my main research topics?",
  {
    ragEnabled: true,
    ragK: 10, // Retrieve top 10 contexts
    ragThreshold: 0.8, // Higher similarity threshold
  }
);
```

---

### Example 4: Disable RAG for Specific Sessions

```typescript
// Disable RAG for this session
const { sessionId } = await sessionManager.startSession(
  modelHash,
  hostAddress,
  {
    depositAmount: "1.0",
    pricePerToken: 200,
    duration: 3600,
    proofInterval: 100,
    enableRAG: false, // Explicitly disable RAG
  }
);

// Or disable for specific prompts
const response = await sessionManager.sendPrompt(
  sessionId,
  "Simple calculation: 2+2",
  { ragEnabled: false }
);
```

---

## Testing Strategy

### Unit Tests

Test the RAG manager in isolation:

```typescript
// tests/rag/VectorRAGManager.test.ts
import { VectorRAGManager } from "../../src/rag/VectorRAGManager";

describe("VectorRAGManager", () => {
  let ragManager: VectorRAGManager;

  beforeEach(() => {
    ragManager = new VectorRAGManager({
      s5Config: { portalUrl: "http://localhost:5524" },
    });
  });

  afterEach(async () => {
    await ragManager.cleanup();
  });

  it("should initialize session", async () => {
    await ragManager.initializeSession(
      "test-session-1",
      "test seed phrase",
      undefined // No existing vectors
    );

    const stats = ragManager.getStats();
    expect(stats).toBeDefined();
    expect(stats.vectorCount).toBe(0);
  });

  it("should add and search vectors", async () => {
    await ragManager.initializeSession(
      "test-session-2",
      "test seed",
      undefined
    );

    // Add test vectors
    await ragManager.addVectors([
      {
        id: "vec1",
        vector: new Array(384).fill(0.1),
        metadata: { text: "Test document 1" },
      },
      {
        id: "vec2",
        vector: new Array(384).fill(0.2),
        metadata: { text: "Test document 2" },
      },
    ]);

    // Search
    const queryVector = new Array(384).fill(0.15);
    const results = await ragManager.retrieveContext(queryVector, 2);

    expect(results.length).toBeGreaterThan(0);
    expect(results[0]).toHaveProperty("text");
    expect(results[0]).toHaveProperty("score");
  });

  it("should cleanup properly", async () => {
    await ragManager.initializeSession(
      "test-session-3",
      "test seed",
      undefined
    );
    await ragManager.cleanup();

    // Should throw after cleanup
    await expect(
      ragManager.retrieveContext(new Array(384).fill(0), 5)
    ).rejects.toThrow("RAG session not initialized");
  });
});
```

---

### Integration Tests

Test full RAG flow with SessionManager:

```typescript
// tests/integration/rag-session.test.ts
import { FabstirSDKCore } from "../../src";

describe("RAG Integration", () => {
  let sdk: FabstirSDKCore;
  let sessionManager: SessionManager;

  beforeAll(async () => {
    sdk = new FabstirSDKCore({
      rpcUrl: process.env.TEST_RPC_URL,
      contractAddresses: {
        /* test contracts */
      },
    });

    await sdk.authenticate("privateKey", {
      privateKey: process.env.TEST_PRIVATE_KEY,
    });

    sessionManager = sdk.getSessionManager();
  });

  it("should handle full RAG session lifecycle", async () => {
    // Start session with RAG
    const { sessionId } = await sessionManager.startSession(
      testModelHash,
      testHostAddress,
      {
        depositAmount: "1.0",
        pricePerToken: 200,
        duration: 3600,
        proofInterval: 100,
        enableRAG: true,
      }
    );

    expect(sessionId).toBeDefined();

    // Send prompt (should work even with no vectors)
    const response = await sessionManager.sendPrompt(sessionId, "Test query");

    expect(response).toBeDefined();

    // End session
    await sessionManager.endSession(sessionId);
  }, 60000); // 60s timeout

  it("should handle concurrent sessions", async () => {
    const sessions = await Promise.all([
      sessionManager.startSession(model, host, config),
      sessionManager.startSession(model, host, config),
      sessionManager.startSession(model, host, config),
    ]);

    expect(sessions).toHaveLength(3);

    // Cleanup
    await Promise.all(
      sessions.map((s) => sessionManager.endSession(s.sessionId))
    );
  });
});
```

---

### Memory Leak Tests

Ensure proper cleanup:

```typescript
// tests/performance/memory-leak.test.ts
describe("Memory Leak Detection", () => {
  it("should not leak memory across sessions", async () => {
    const iterations = 100;

    for (let i = 0; i < iterations; i++) {
      const ragManager = new VectorRAGManager({ s5Config: {} });

      await ragManager.initializeSession(
        `session-${i}`,
        "test seed",
        undefined
      );

      await ragManager.addVectors([
        {
          id: `vec-${i}`,
          vector: new Array(384).fill(Math.random()),
          metadata: { text: `Document ${i}` },
        },
      ]);

      // CRITICAL: Cleanup
      await ragManager.cleanup();

      // Force garbage collection (if --expose-gc flag set)
      if (global.gc) {
        global.gc();
      }
    }

    // Memory should stabilize, not grow linearly
    // (Actual assertion depends on your testing framework)
  });
});
```

---

## Security Considerations

### Data Isolation

1. **Per-User Sessions:**

   - Each user gets a fresh `VectorDBSession` instance
   - No shared state between users
   - Session identified by unique `sessionId`

2. **Memory Cleanup:**

   - ALWAYS call `session.destroy()` when session ends
   - Use try/finally to ensure cleanup happens
   - Host becomes stateless after cleanup

3. **Seed Phrase Handling:**
   - User's seed phrase passed to native module
   - Used only for S5 authentication
   - Never logged or persisted by host
   - Cleared from memory with session

### Encryption Layer

All user data in transit is encrypted:

- User ↔ Host: XChaCha20-Poly1305
- Host ↔ S5: HTTPS/TLS
- S5 ↔ Sia: Proof of Storage verification

### Best Practices

```typescript
// ✅ GOOD: Always use try/finally
async function handleSession() {
  const session = await VectorDBSession.create(config);

  try {
    await session.loadUserVectors(cid);
    const results = await session.search(query, k);
    return results;
  } finally {
    await session.destroy(); // CRITICAL
  }
}

// ❌ BAD: No cleanup guarantee
async function handleSessionBad() {
  const session = await VectorDBSession.create(config);
  await session.loadUserVectors(cid);
  const results = await session.search(query, k);
  await session.destroy(); // Might not execute if error above
  return results;
}
```

---

## Performance Characteristics

### Load Times

| Vector Count | Lazy Load | Full Load | Memory Usage |
| ------------ | --------- | --------- | ------------ |
| 10K vectors  | ~500ms    | ~1s       | ~20 MB       |
| 100K vectors | ~2s       | ~5s       | ~200 MB      |
| 1M vectors   | ~5s       | ~30s      | ~2 GB        |
| 10M vectors  | ~20s      | ~5min     | ~20 GB       |

### Search Latency

| Vector Count | HNSW (Recent) | IVF (Historical) | Hybrid  |
| ------------ | ------------- | ---------------- | ------- |
| 10K          | < 5ms         | < 10ms           | < 10ms  |
| 100K         | < 10ms        | < 20ms           | < 20ms  |
| 1M           | < 20ms        | < 50ms           | < 50ms  |
| 10M          | < 50ms        | < 100ms          | < 100ms |

### Memory Budgets

Recommended memory budgets per session:

- **Small datasets (< 100K vectors):** 256 MB
- **Medium datasets (100K - 1M vectors):** 512 MB
- **Large datasets (1M - 10M vectors):** 2-4 GB

### Optimization Tips

1. **Use Lazy Loading:** Default behavior, best for large datasets
2. **Set Memory Budgets:** Prevent OOM on shared hosts
3. **Batch Operations:** Add vectors in batches, not individually
4. **Threshold Tuning:** Higher thresholds = fewer results = faster

---

## Troubleshooting

### Common Issues

#### 1. Module Not Found

**Error:**

```
Error: Cannot find module '@fabstir/vector-db-native'
```

**Solution:**

```bash
# Verify installation
npm list @fabstir/vector-db-native

# Reinstall
npm install /path/to/fabstir-vector-db-native-0.1.0.tgz

# Check platform compatibility
node -p "process.platform + '-' + process.arch"
# Should output: linux-x64
```

---

#### 2. S5 Connection Failed

**Error:**

```
VectorDBError: Failed to connect to S5 portal
```

**Solution:**

```typescript
// Check S5 portal is running
const response = await fetch("http://localhost:5524/s5/health");
console.log(await response.text());

// Use correct portal URL
const session = await VectorDBSession.create({
  s5Portal: "http://localhost:5524", // Ensure correct port
  // ...
});
```

---

#### 3. Out of Memory

**Error:**

```
VectorDBError: Memory budget exceeded
```

**Solution:**

```typescript
// Reduce memory budget
const session = await VectorDBSession.create({
  // ...
  memoryBudgetMB: 256, // Reduce from 512
});

// Or use lazy loading
await session.loadUserVectors(cid, {
  lazyLoad: true, // Load IVF on-demand
});
```

---

#### 4. Session Not Cleaned Up

**Symptom:** Memory usage grows across sessions

**Solution:**

```typescript
// Always use try/finally
async function handleSession() {
  const ragManager = new VectorRAGManager(config);

  try {
    await ragManager.initializeSession(...);
    // ... use session
  } finally {
    await ragManager.cleanup();  // CRITICAL
  }
}
```

---

#### 5. Embedding Generation Failed

**Error:**

```
Error: Embedding generation not implemented
```

**Solution:**

```typescript
// Implement embedding service integration
import { EmbeddingService } from "./embeddings/EmbeddingService";

const embeddingService = new EmbeddingService("http://localhost:8081/embed");
const embedding = await embeddingService.generateEmbedding(text);
```

---

### Debug Mode

Enable debug logging:

```typescript
// Set environment variable
process.env.VECTOR_DB_DEBUG = "true";

// Or in code
const session = await VectorDBSession.create({
  // ...
  debug: true,
});
```

---

## Appendix

### Type Definitions

Full TypeScript types are available in the package:

```typescript
import type {
  VectorDBSession,
  VectorDBConfig,
  LoadOptions,
  SearchOptions,
  VectorInput,
  SearchResult,
  SessionStats,
  VectorDBError,
} from "@fabstir/vector-db-native";
```

### Environment Variables

| Variable               | Description              | Default                       |
| ---------------------- | ------------------------ | ----------------------------- |
| `S5_PORTAL_URL`        | S5 portal endpoint       | `http://localhost:5524`       |
| `EMBEDDING_ENDPOINT`   | Embedding model endpoint | `http://localhost:8081/embed` |
| `VECTOR_DB_DEBUG`      | Enable debug logging     | `false`                       |
| `RAG_MEMORY_BUDGET_MB` | Default memory budget    | `512`                         |

---

## Support

For issues with the vector DB native module, contact the fabstir-vectordb team.

For SDK integration questions, refer to the main SDK documentation at `docs/sdk-reference/SDK_API.md`.

---

**Last Updated:** 2025-01-26
**Version:** 1.0.0
**Status:** Implementation Ready
