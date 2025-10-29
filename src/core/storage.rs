// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};
use tokio::time::sleep;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Circuit breaker is open")]
    CircuitBreakerOpen,
}

#[async_trait]
pub trait S5Storage: Send + Sync {
    async fn get(&self, path: &str) -> Result<Option<Vec<u8>>, StorageError>;
    async fn put(&self, path: &str, data: Vec<u8>) -> Result<(), StorageError>;
    async fn delete(&self, path: &str) -> Result<(), StorageError>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>, StorageError>;
}

// Cache entry with timestamp
struct CacheEntry {
    data: Vec<u8>,
    timestamp: Instant,
    size: usize,
}

pub struct CachedS5Storage<T> {
    inner: T,
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    access_order: Arc<RwLock<Vec<String>>>,
    capacity: usize,
    ttl: Option<Duration>,
    memory_limit: Option<usize>,
    stats: Arc<RwLock<CacheStats>>,
}

#[derive(Clone, Debug)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
    pub entries: usize,
    pub memory_bytes: usize,
}

impl<T: S5Storage> CachedS5Storage<T> {
    pub fn new(inner: T, capacity: usize) -> Self {
        Self {
            inner,
            cache: Arc::new(RwLock::new(HashMap::new())),
            access_order: Arc::new(RwLock::new(Vec::new())),
            capacity,
            ttl: None,
            memory_limit: None,
            stats: Arc::new(RwLock::new(CacheStats {
                hits: 0,
                misses: 0,
                entries: 0,
                memory_bytes: 0,
            })),
        }
    }

    pub fn with_ttl(inner: T, capacity: usize, ttl: Duration) -> Self {
        let mut cache = Self::new(inner, capacity);
        cache.ttl = Some(ttl);
        cache
    }

    pub fn with_memory_limit(inner: T, memory_limit: usize) -> Self {
        Self {
            inner,
            cache: Arc::new(RwLock::new(HashMap::new())),
            access_order: Arc::new(RwLock::new(Vec::new())),
            capacity: usize::MAX,
            ttl: None,
            memory_limit: Some(memory_limit),
            stats: Arc::new(RwLock::new(CacheStats {
                hits: 0,
                misses: 0,
                entries: 0,
                memory_bytes: 0,
            })),
        }
    }

    pub async fn stats(&self) -> CacheStats {
        self.stats.read().await.clone()
    }

    async fn update_lru(&self, key: &str) {
        let mut order = self.access_order.write().await;
        order.retain(|k| k != key);
        order.push(key.to_string());
    }

    async fn evict_if_needed(&self) {
        let mut cache = self.cache.write().await;
        let mut order = self.access_order.write().await;
        let mut stats = self.stats.write().await;

        // Check capacity limit
        while cache.len() >= self.capacity && !order.is_empty() {
            let key = order.remove(0);
            if let Some(entry) = cache.remove(&key) {
                stats.entries -= 1;
                stats.memory_bytes -= entry.size;
            }
        }

        // Check memory limit
        if let Some(limit) = self.memory_limit {
            while stats.memory_bytes > limit && !order.is_empty() {
                let key = order.remove(0);
                if let Some(entry) = cache.remove(&key) {
                    stats.entries -= 1;
                    stats.memory_bytes -= entry.size;
                }
            }
        }
    }

    async fn evict_for_size(&self, new_size: usize) {
        let mut cache = self.cache.write().await;
        let mut order = self.access_order.write().await;
        let mut stats = self.stats.write().await;

        // Check if adding new_size would exceed memory limit
        if let Some(limit) = self.memory_limit {
            while stats.memory_bytes + new_size > limit && !order.is_empty() {
                let key = order.remove(0);
                if let Some(entry) = cache.remove(&key) {
                    stats.entries -= 1;
                    stats.memory_bytes -= entry.size;
                }
            }
        }
    }

    async fn is_expired(&self, entry: &CacheEntry) -> bool {
        if let Some(ttl) = self.ttl {
            entry.timestamp.elapsed() > ttl
        } else {
            false
        }
    }
}

#[async_trait]
impl<T: S5Storage> S5Storage for CachedS5Storage<T> {
    async fn get(&self, path: &str) -> Result<Option<Vec<u8>>, StorageError> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.get(path) {
                if !self.is_expired(entry).await {
                    let data = entry.data.clone();
                    drop(cache);

                    let mut stats = self.stats.write().await;
                    stats.hits += 1;
                    drop(stats);

                    self.update_lru(path).await;
                    return Ok(Some(data));
                }
            }
        }

        // Cache miss - fetch from inner storage
        let result = self.inner.get(path).await?;

        let mut stats = self.stats.write().await;
        stats.misses += 1;
        drop(stats);

        // Update cache if data was found
        if let Some(data) = &result {
            let entry = CacheEntry {
                size: data.len(),
                data: data.clone(),
                timestamp: Instant::now(),
            };

            self.evict_if_needed().await;

            let mut cache = self.cache.write().await;
            let mut stats = self.stats.write().await;
            cache.insert(path.to_string(), entry);
            stats.entries += 1;
            stats.memory_bytes += data.len();
            drop(cache);
            drop(stats);

            self.update_lru(path).await;
        }

        Ok(result)
    }

    async fn put(&self, path: &str, data: Vec<u8>) -> Result<(), StorageError> {
        // Write through to storage
        self.inner.put(path, data.clone()).await?;

        // Update cache
        let entry = CacheEntry {
            size: data.len(),
            data,
            timestamp: Instant::now(),
        };

        let mut cache = self.cache.write().await;
        let mut stats = self.stats.write().await;

        // Check if we're updating an existing entry
        let is_update = cache.contains_key(path);

        // Remove old entry if exists
        if let Some(old_entry) = cache.remove(path) {
            stats.memory_bytes -= old_entry.size;
            stats.entries -= 1;
        }

        // Only evict if we're adding a new entry (not updating)
        if !is_update {
            drop(cache);
            drop(stats);
            self.evict_for_size(entry.size).await;
            self.evict_if_needed().await;
            cache = self.cache.write().await;
            stats = self.stats.write().await;
        }

        cache.insert(path.to_string(), entry);
        stats.entries += 1;
        stats.memory_bytes += cache.get(path).unwrap().size;
        drop(cache);
        drop(stats);

        self.update_lru(path).await;

        Ok(())
    }

    async fn delete(&self, path: &str) -> Result<(), StorageError> {
        // Delete from storage
        self.inner.delete(path).await?;

        // Remove from cache
        let mut cache = self.cache.write().await;
        let mut stats = self.stats.write().await;
        if let Some(entry) = cache.remove(path) {
            stats.entries -= 1;
            stats.memory_bytes -= entry.size;
        }

        let mut order = self.access_order.write().await;
        order.retain(|k| k != path);

        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
        self.inner.list(prefix).await
    }
}

// Retry configuration
pub struct RetryConfig {
    pub max_attempts: usize,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub exponential_base: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            exponential_base: 2.0,
        }
    }
}

pub struct RetryS5Storage<T> {
    inner: T,
    config: RetryConfig,
    circuit_breaker: Option<CircuitBreaker>,
    use_jitter: bool,
}

struct CircuitBreaker {
    failure_threshold: usize,
    reset_timeout: Duration,
    failures: Arc<Mutex<usize>>,
    last_failure: Arc<Mutex<Option<Instant>>>,
}

impl CircuitBreaker {
    fn new(threshold: usize, timeout: Duration) -> Self {
        Self {
            failure_threshold: threshold,
            reset_timeout: timeout,
            failures: Arc::new(Mutex::new(0)),
            last_failure: Arc::new(Mutex::new(None)),
        }
    }

    async fn is_open(&self) -> bool {
        let failures = *self.failures.lock().await;
        if failures >= self.failure_threshold {
            if let Some(last) = *self.last_failure.lock().await {
                return last.elapsed() < self.reset_timeout;
            }
        }
        false
    }

    async fn record_success(&self) {
        *self.failures.lock().await = 0;
        *self.last_failure.lock().await = None;
    }

    async fn record_failure(&self) {
        let mut failures = self.failures.lock().await;
        *failures += 1;
        *self.last_failure.lock().await = Some(Instant::now());
    }
}

impl<T: S5Storage> RetryS5Storage<T> {
    pub fn new(inner: T, max_retries: usize) -> Self {
        let mut config = RetryConfig::default();
        config.max_attempts = max_retries;
        Self {
            inner,
            config,
            circuit_breaker: None,
            use_jitter: false,
        }
    }

    pub fn with_config(inner: T, config: RetryConfig) -> Self {
        Self {
            inner,
            config,
            circuit_breaker: None,
            use_jitter: false,
        }
    }

    pub fn with_circuit_breaker(
        inner: T,
        failure_threshold: usize,
        reset_timeout: Duration,
    ) -> Self {
        let mut storage = Self::new(inner, 3);
        storage.circuit_breaker = Some(CircuitBreaker::new(failure_threshold, reset_timeout));
        storage
    }

    pub fn with_jitter(inner: T, max_attempts: usize) -> Self {
        let mut storage = Self::new(inner, max_attempts);
        storage.use_jitter = true;
        storage
    }

    async fn retry_with_backoff<F, Fut, R>(&self, mut operation: F) -> Result<R, StorageError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<R, StorageError>>,
    {
        // Check circuit breaker
        if let Some(ref breaker) = self.circuit_breaker {
            if breaker.is_open().await {
                return Err(StorageError::CircuitBreakerOpen);
            }
        }

        let mut attempts = 0;
        let mut delay = self.config.initial_delay;

        loop {
            attempts += 1;

            match operation().await {
                Ok(result) => {
                    if let Some(ref breaker) = self.circuit_breaker {
                        breaker.record_success().await;
                    }
                    return Ok(result);
                }
                Err(e) if attempts >= self.config.max_attempts => {
                    if let Some(ref breaker) = self.circuit_breaker {
                        breaker.record_failure().await;
                    }
                    return Err(e);
                }
                Err(_) => {
                    if let Some(ref breaker) = self.circuit_breaker {
                        breaker.record_failure().await;
                    }

                    // Apply jitter if enabled
                    let mut actual_delay = delay;
                    if self.use_jitter {
                        let jitter = Duration::from_millis(
                            (rand::random::<f64>() * delay.as_millis() as f64 * 0.3) as u64,
                        );
                        actual_delay = delay + jitter;
                    }

                    sleep(actual_delay).await;

                    // Calculate next delay with exponential backoff
                    let next_delay = Duration::from_millis(
                        (delay.as_millis() as f64 * self.config.exponential_base) as u64,
                    );
                    delay = next_delay.min(self.config.max_delay);
                }
            }
        }
    }
}

#[async_trait]
impl<T: S5Storage> S5Storage for RetryS5Storage<T> {
    async fn get(&self, path: &str) -> Result<Option<Vec<u8>>, StorageError> {
        let path = path.to_string();
        self.retry_with_backoff(|| {
            let inner = &self.inner;
            let path = path.clone();
            async move { inner.get(&path).await }
        })
        .await
    }

    async fn put(&self, path: &str, data: Vec<u8>) -> Result<(), StorageError> {
        let path = path.to_string();
        self.retry_with_backoff(|| {
            let inner = &self.inner;
            let path = path.clone();
            let data = data.clone();
            async move { inner.put(&path, data).await }
        })
        .await
    }

    async fn delete(&self, path: &str) -> Result<(), StorageError> {
        let path = path.to_string();
        self.retry_with_backoff(|| {
            let inner = &self.inner;
            let path = path.clone();
            async move { inner.delete(&path).await }
        })
        .await
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
        let prefix = prefix.to_string();
        self.retry_with_backoff(|| {
            let inner = &self.inner;
            let prefix = prefix.clone();
            async move { inner.list(&prefix).await }
        })
        .await
    }
}

// Batch storage configuration
pub struct BatchConfig {
    pub max_batch_size: usize,
    pub flush_interval: Duration,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 100,
            flush_interval: Duration::from_secs(5),
        }
    }
}

pub struct BatchS5Storage<T> {
    inner: Arc<T>,
    config: BatchConfig,
    write_buffer: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    delete_buffer: Arc<Mutex<Vec<String>>>,
}

impl<T: S5Storage + 'static> BatchS5Storage<T> {
    pub fn new(inner: T, batch_size: usize) -> Self {
        let mut config = BatchConfig::default();
        config.max_batch_size = batch_size;
        Self::with_config(inner, config)
    }

    pub fn with_config(inner: T, config: BatchConfig) -> Self {
        let storage = Self {
            inner: Arc::new(inner),
            config,
            write_buffer: Arc::new(Mutex::new(HashMap::new())),
            delete_buffer: Arc::new(Mutex::new(Vec::new())),
        };

        // Start background flush task
        let inner_clone = storage.inner.clone();
        let write_buffer_clone = storage.write_buffer.clone();
        let delete_buffer_clone = storage.delete_buffer.clone();
        let flush_interval = storage.config.flush_interval;

        tokio::spawn(async move {
            loop {
                sleep(flush_interval).await;
                Self::flush_buffers(&inner_clone, &write_buffer_clone, &delete_buffer_clone).await;
            }
        });

        storage
    }

    pub fn inner_storage(&self) -> &T {
        &self.inner
    }

    async fn flush_buffers(
        inner: &Arc<T>,
        write_buffer: &Arc<Mutex<HashMap<String, Vec<u8>>>>,
        delete_buffer: &Arc<Mutex<Vec<String>>>,
    ) {
        // Flush writes
        let writes = {
            let mut buffer = write_buffer.lock().await;
            std::mem::take(&mut *buffer)
        };

        for (path, data) in writes {
            let _ = inner.put(&path, data).await;
        }

        // Flush deletes
        let deletes = {
            let mut buffer = delete_buffer.lock().await;
            std::mem::take(&mut *buffer)
        };

        for path in deletes {
            let _ = inner.delete(&path).await;
        }
    }

    async fn check_and_flush(&self) {
        let should_flush = {
            let write_buffer = self.write_buffer.lock().await;
            let delete_buffer = self.delete_buffer.lock().await;
            write_buffer.len() + delete_buffer.len() >= self.config.max_batch_size
        };

        if should_flush {
            Self::flush_buffers(&self.inner, &self.write_buffer, &self.delete_buffer).await;
        }
    }
}

#[async_trait]
impl<T: S5Storage + 'static> S5Storage for BatchS5Storage<T> {
    async fn get(&self, path: &str) -> Result<Option<Vec<u8>>, StorageError> {
        // Check write buffer first
        {
            let buffer = self.write_buffer.lock().await;
            if let Some(data) = buffer.get(path) {
                return Ok(Some(data.clone()));
            }
        }

        // Check if it's in delete buffer
        {
            let buffer = self.delete_buffer.lock().await;
            if buffer.contains(&path.to_string()) {
                return Ok(None);
            }
        }

        // Fall back to inner storage
        self.inner.get(path).await
    }

    async fn put(&self, path: &str, data: Vec<u8>) -> Result<(), StorageError> {
        {
            let mut write_buffer = self.write_buffer.lock().await;
            write_buffer.insert(path.to_string(), data);

            // Remove from delete buffer if present
            let mut delete_buffer = self.delete_buffer.lock().await;
            delete_buffer.retain(|p| p != path);
        }

        self.check_and_flush().await;
        Ok(())
    }

    async fn delete(&self, path: &str) -> Result<(), StorageError> {
        {
            let mut delete_buffer = self.delete_buffer.lock().await;
            delete_buffer.push(path.to_string());

            // Remove from write buffer if present
            let mut write_buffer = self.write_buffer.lock().await;
            write_buffer.remove(path);
        }

        self.check_and_flush().await;
        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
        // For list operations, we need to flush first to ensure consistency
        Self::flush_buffers(&self.inner, &self.write_buffer, &self.delete_buffer).await;
        self.inner.list(prefix).await
    }
}

// Mock S5 storage for testing
#[derive(Clone)]
pub struct MockS5Storage {
    data: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    call_count: Arc<RwLock<HashMap<String, usize>>>,
}

impl MockS5Storage {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
            call_count: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl S5Storage for MockS5Storage {
    async fn get(&self, path: &str) -> Result<Option<Vec<u8>>, StorageError> {
        let mut counts = self.call_count.write().await;
        *counts.entry(path.to_string()).or_insert(0) += 1;

        let data = self.data.read().await;
        Ok(data.get(path).cloned())
    }

    async fn put(&self, path: &str, data: Vec<u8>) -> Result<(), StorageError> {
        let mut storage = self.data.write().await;
        storage.insert(path.to_string(), data);
        Ok(())
    }

    async fn delete(&self, path: &str) -> Result<(), StorageError> {
        let mut storage = self.data.write().await;
        storage.remove(path);
        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
        let data = self.data.read().await;
        Ok(data
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }
}
