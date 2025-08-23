## 📝 Updates for fabstir-vectordb/IMPLEMENTATION.md

```markdown
### Phase 5: Multi-Tenant RAG Support (Future - Post-MVP)

#### Overview
Enable fabstir-vectordb to support temporary user-specific sessions while maintaining security model. This is planned but NOT implemented until after MVP ships.

#### Background
- MVP uses simple context passing (no vectordb needed)
- Phase 2 adds compaction (vectordb for embedding generation only)
- Phase 3 requires full RAG with user-specific vector search

#### Architectural Constraints
- Seed phrases only via environment variables (security requirement)
- Cannot change seed at runtime (by design)
- Must maintain data isolation between users
- Hosts are stateless workers

#### Proposed Solution: Session-Based Instances

##### Option 1: Container Orchestration (Recommended)
```yaml
# Dynamic container per user session
fabstir-vectordb-session-{user-id}:
  image: fabstir-vectordb:latest
  environment:
    - S5_SEED_PHRASE=${USER_DERIVED_SEED}
    - MODE=read-only
    - SESSION_ID=${SESSION_ID}
    - TTL=1800  # 30 minutes
  resources:
    limits:
      memory: 512Mi
      cpu: 0.5
```

##### Option 2: Process Pool with Isolation
```rust
// Future implementation concept
pub struct VectorDBPool {
    instances: HashMap<SessionId, VectorDBProcess>,
    max_instances: usize,
}

impl VectorDBPool {
    async fn create_session(
        &mut self,
        user_seed: String,
        session_id: SessionId,
    ) -> Result<VectorDBHandle> {
        // Spawn new process with user's seed
        let process = Command::new("./fabstir-vectordb")
            .env("S5_SEED_PHRASE", user_seed)
            .env("PORT", get_free_port())
            .spawn()?;
        
        // Track instance
        self.instances.insert(session_id, process);
        
        Ok(VectorDBHandle { session_id, port })
    }
}
```

#### Implementation Phases

##### Phase 3.1: Embedding Generation Only (For Compaction)
- [ ] Add `/embeddings/generate` endpoint
- [ ] No storage required (compute only)
- [ ] Single shared instance sufficient
- [ ] Mock S5 storage backend

##### Phase 3.2: Read-Only Sessions (For RAG)
- [ ] Support read-only mode flag
- [ ] Load embeddings from S5 on startup
- [ ] Build in-memory index
- [ ] Provide search API
- [ ] Auto-terminate after TTL

##### Phase 3.3: Delegation Support
- [ ] Accept delegation tokens
- [ ] Validate token signatures
- [ ] Scope access to user's namespace
- [ ] Audit access logs

#### API Extensions (Future)

##### Session Management Endpoints
```rust
// POST /sessions/create
pub struct CreateSessionRequest {
    pub delegation_token: String,  // User's delegated access
    pub mode: SessionMode,         // ReadOnly, Compute
    pub ttl_seconds: u32,          // Session duration
}

// GET /sessions/{id}/search
pub struct SearchRequest {
    pub query: String,
    pub top_k: usize,
    pub threshold: f32,
}

// DELETE /sessions/{id}
// Cleanup session and resources
```

#### Security Considerations

##### Data Isolation
- [ ] Each session runs in isolated process/container
- [ ] No shared memory between sessions
- [ ] Network isolation between instances
- [ ] Automatic cleanup on expiration

##### Resource Limits
- [ ] Max sessions per host
- [ ] Memory limits per session
- [ ] CPU quotas
- [ ] Disk usage monitoring

#### Performance Optimizations

##### Caching Strategy
```rust
// Future: Cache frequently accessed embeddings
pub struct EmbeddingCache {
    hot_cache: LRU<String, Vec<f32>>,
    warm_cache: DiskCache,
}
```

##### Session Pooling
- [ ] Pre-warm common models
- [ ] Reuse terminated sessions
- [ ] Connection pooling to S5
- [ ] Batch embedding operations

#### Migration Path

##### Current State (MVP)
```
No vectordb needed → Simple context passing
```

##### Phase 2 (Compaction)
```
Shared vectordb → Embedding generation only
```

##### Phase 3 (RAG)
```
Per-user sessions → Full vector search
```

#### Success Criteria (When Implemented)
- [ ] User data remains isolated
- [ ] Sessions auto-terminate
- [ ] Resource usage bounded
- [ ] No seed phrase exposure
- [ ] Sub-second search latency
- [ ] Graceful degradation

#### Testing Strategy (Future)

##### Security Tests
- [ ] Verify session isolation
- [ ] Test delegation validation
- [ ] Attempt cross-session access
- [ ] Verify cleanup on crash

##### Performance Tests
- [ ] Measure session creation time
- [ ] Test concurrent sessions
- [ ] Load test search operations
- [ ] Memory leak detection

##### Integration Tests
- [ ] End-to-end RAG flow
- [ ] Session expiration handling
- [ ] S5 connectivity under load
- [ ] Graceful shutdown

#### Note
This phase is intentionally designed but NOT implemented until:
1. MVP ships with simple context passing
2. Compaction feature is validated
3. User demand justifies complexity
4. Security model is thoroughly reviewed
```

## 🎯 Summary of Changes

### Timeline
1. **Week 1 (MVP)**: Ship with context passing, no vectordb
2. **Week 2-3**: Add compaction feature
3. **Week 4+**: Implement RAG if needed

### Key Decisions
- **MVP**: Skip vectordb entirely, use context passing
- **Compaction**: Brilliant user-controlled upgrade path
- **RAG**: Only when truly needed, with careful security
