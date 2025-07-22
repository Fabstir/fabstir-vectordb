use vector_db::core::storage::*;
use vector_db::core::types::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::time::sleep;

#[cfg(test)]
mod cache_tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_eviction_lru() {
        let base = MockS5Storage::new();
        let cached = CachedS5Storage::new(base, 3); // Small cache size
        
        // Fill cache
        for i in 0..4 {
            let path = format!("/test/{}", i);
            let data = format!("data_{}", i).into_bytes();
            cached.put(&path, data).await.unwrap();
        }

        // Access items 0, 1, 2 to make them recently used
        for i in 0..3 {
            let path = format!("/test/{}", i);
            cached.get(&path).await.unwrap();
        }

        let stats_before = cached.stats().await;
        println!("Stats before adding item 4: {:?}", stats_before);

        // Item 3 should be evicted when we add a new item
        cached.put("/test/4", b"data_4".to_vec()).await.unwrap();

        // Check cache stats
        let stats = cached.stats().await;
        println!("Stats after adding item 4: {:?}", stats);
        assert_eq!(stats.entries, 3);
        
        // Item 3 should cause a cache miss (evicted)
        cached.get("/test/3").await.unwrap();
        let stats = cached.stats().await;
        println!("Stats after trying to get item 3: {:?}", stats);
        assert_eq!(stats.misses, stats_before.misses + 1);
    }

    #[tokio::test]
    async fn test_cache_ttl_expiration() {
        let base = MockS5Storage::new();
        let cached = CachedS5Storage::with_ttl(base, 100, Duration::from_millis(100));
        
        let path = "/ttl/test";
        let data = b"expires soon".to_vec();
        
        // Put and immediately get (should be cached)
        cached.put(path, data.clone()).await.unwrap();
        let result1 = cached.get(path).await.unwrap();
        assert_eq!(result1, Some(data.clone()));
        
        let stats1 = cached.stats().await;
        assert_eq!(stats1.hits, 1);
        
        // Wait for TTL to expire
        sleep(Duration::from_millis(150)).await;
        
        // Should cause cache miss
        let result2 = cached.get(path).await.unwrap();
        assert_eq!(result2, Some(data));
        
        let stats2 = cached.stats().await;
        assert_eq!(stats2.misses, 1);
    }

    #[tokio::test]
    async fn test_cache_memory_limit() {
        let base = MockS5Storage::new();
        // 1KB memory limit
        let cached = CachedS5Storage::with_memory_limit(base, 1024);
        
        // Add items until memory limit exceeded
        for i in 0..10 {
            let path = format!("/mem/{}", i);
            let data = vec![0u8; 200]; // 200 bytes each
            cached.put(&path, data).await.unwrap();
            let stats = cached.stats().await;
            println!("After adding item {}: entries={}, memory={}", i, stats.entries, stats.memory_bytes);
        }
        
        let stats = cached.stats().await;
        println!("Final stats: entries={}, memory={}", stats.entries, stats.memory_bytes);
        assert!(stats.memory_bytes <= 1024);
        assert!(stats.entries <= 5); // Should have evicted some
    }

    #[tokio::test]
    async fn test_cache_invalidation() {
        let base = MockS5Storage::new();
        let cached = CachedS5Storage::new(base, 100);
        
        let path = "/invalidate/test";
        let data1 = b"version1".to_vec();
        let data2 = b"version2".to_vec();
        
        // Cache version 1
        cached.put(path, data1.clone()).await.unwrap();
        assert_eq!(cached.get(path).await.unwrap(), Some(data1));
        
        // Update should invalidate cache
        cached.put(path, data2.clone()).await.unwrap();
        assert_eq!(cached.get(path).await.unwrap(), Some(data2));
        
        // Delete should invalidate cache
        cached.delete(path).await.unwrap();
        assert_eq!(cached.get(path).await.unwrap(), None);
    }
}

#[cfg(test)]
mod retry_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn test_retry_exponential_backoff() {
        let attempt_count = Arc::new(AtomicUsize::new(0));
        let storage = FailingStorage::new(3, attempt_count.clone());
        let retry = RetryS5Storage::with_config(
            storage,
            RetryConfig {
                max_attempts: 5,
                initial_delay: Duration::from_millis(10),
                max_delay: Duration::from_millis(100),
                exponential_base: 2.0,
            }
        );
        
        let start = Instant::now();
        retry.put("/test", b"data".to_vec()).await.unwrap();
        let duration = start.elapsed();
        
        // Should have retried 3 times with exponential backoff
        assert_eq!(attempt_count.load(Ordering::SeqCst), 4); // 1 initial + 3 retries
        // Total delay should be roughly 10 + 20 + 40 = 70ms
        assert!(duration.as_millis() >= 60 && duration.as_millis() < 150);
    }

    #[tokio::test]
    async fn test_retry_circuit_breaker() {
        let storage = FailingStorage::new(100, Arc::new(AtomicUsize::new(0)));
        let retry = RetryS5Storage::with_circuit_breaker(storage, 3, Duration::from_secs(1));
        
        // First 3 failures should trip the circuit breaker
        for i in 0..3 {
            let path = format!("/test/{}", i);
            let result = retry.put(&path, b"data".to_vec()).await;
            assert!(result.is_err());
        }
        
        // Circuit should be open, failing fast
        let start = Instant::now();
        let result = retry.put("/test/4", b"data".to_vec()).await;
        assert!(result.is_err());
        assert!(start.elapsed() < Duration::from_millis(10)); // Failed fast
        
        match result.unwrap_err() {
            StorageError::CircuitBreakerOpen => {},
            _ => panic!("Expected CircuitBreakerOpen error"),
        }
    }

    #[tokio::test]
    async fn test_retry_with_jitter() {
        let attempt_times = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let times_clone = attempt_times.clone();
        
        let storage = TimingStorage::new(3, times_clone);
        let retry = RetryS5Storage::with_jitter(storage, 5);
        
        retry.put("/test", b"data".to_vec()).await.unwrap();
        
        let times = attempt_times.lock().await;
        assert_eq!(times.len(), 4); // 1 initial + 3 retries
        
        // Check that delays have jitter (not exactly exponential)
        for i in 1..times.len() {
            let delay = times[i].duration_since(times[i-1]).unwrap();
            let expected = Duration::from_millis(10 * 2_u64.pow(i as u32 - 1));
            // Jitter should make it different from exact exponential
            assert!(delay.as_millis() as i64 - expected.as_millis() as i64 != 0);
        }
    }
}

#[cfg(test)]
mod batch_tests {
    use super::*;

    #[tokio::test]
    async fn test_batch_write_buffer() {
        let base = MockS5Storage::new();
        let batch = BatchS5Storage::with_config(
            base,
            BatchConfig {
                max_batch_size: 3,
                flush_interval: Duration::from_millis(100),
            }
        );
        
        // Add items without flushing
        for i in 0..2 {
            let path = format!("/batch/{}", i);
            batch.put(&path, vec![i as u8]).await.unwrap();
        }
        
        // Items shouldn't be in storage yet
        let inner = batch.inner_storage();
        assert!(inner.get("/batch/0").await.unwrap().is_none());
        
        // Third item should trigger flush
        batch.put("/batch/2", vec![2]).await.unwrap();
        
        // Now all items should be flushed
        sleep(Duration::from_millis(50)).await;
        for i in 0..3 {
            let path = format!("/batch/{}", i);
            let data = inner.get(&path).await.unwrap();
            assert_eq!(data, Some(vec![i as u8]));
        }
    }

    #[tokio::test]
    async fn test_batch_time_based_flush() {
        let base = MockS5Storage::new();
        let batch = BatchS5Storage::with_config(
            base,
            BatchConfig {
                max_batch_size: 100,
                flush_interval: Duration::from_millis(50),
            }
        );
        
        // Add one item
        batch.put("/timed/test", b"data".to_vec()).await.unwrap();
        
        // Not flushed immediately
        let inner = batch.inner_storage();
        assert!(inner.get("/timed/test").await.unwrap().is_none());
        
        // Wait for timed flush
        sleep(Duration::from_millis(100)).await;
        
        // Should be flushed now
        assert!(inner.get("/timed/test").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_batch_flush_on_read() {
        let base = MockS5Storage::new();
        // Create batch storage with short flush interval for testing
        let batch = BatchS5Storage::with_config(
            base,
            BatchConfig {
                max_batch_size: 10,
                flush_interval: Duration::from_millis(10),
            }
        );
        
        // Write to batch
        batch.put("/read/test", b"data".to_vec()).await.unwrap();
        
        // Reading should see the buffered write
        let result = batch.get("/read/test").await.unwrap();
        assert_eq!(result, Some(b"data".to_vec()));
        
        // Wait for background flush
        let inner = batch.inner_storage();
        sleep(Duration::from_millis(50)).await;
        assert!(inner.get("/read/test").await.unwrap().is_some());
    }
}

// Helper implementations for testing
struct FailingStorage {
    failures_remaining: AtomicUsize,
    attempt_count: Arc<AtomicUsize>,
}

impl FailingStorage {
    fn new(failures: usize, counter: Arc<AtomicUsize>) -> Self {
        Self {
            failures_remaining: AtomicUsize::new(failures),
            attempt_count: counter,
        }
    }
}

#[async_trait::async_trait]
impl S5Storage for FailingStorage {
    async fn get(&self, _path: &str) -> Result<Option<Vec<u8>>, StorageError> {
        self.attempt_count.fetch_add(1, Ordering::SeqCst);
        if self.failures_remaining.fetch_sub(1, Ordering::SeqCst) > 0 {
            Err(StorageError::NetworkError("Simulated failure".into()))
        } else {
            Ok(Some(b"success".to_vec()))
        }
    }

    async fn put(&self, _path: &str, _data: Vec<u8>) -> Result<(), StorageError> {
        self.attempt_count.fetch_add(1, Ordering::SeqCst);
        if self.failures_remaining.load(Ordering::SeqCst) > 0 {
            self.failures_remaining.fetch_sub(1, Ordering::SeqCst);
            Err(StorageError::NetworkError("Simulated failure".into()))
        } else {
            Ok(())
        }
    }

    async fn delete(&self, _path: &str) -> Result<(), StorageError> {
        Ok(())
    }

    async fn list(&self, _prefix: &str) -> Result<Vec<String>, StorageError> {
        Ok(vec![])
    }
}

struct TimingStorage {
    failures_remaining: AtomicUsize,
    attempt_times: Arc<tokio::sync::Mutex<Vec<std::time::SystemTime>>>,
}

impl TimingStorage {
    fn new(failures: usize, times: Arc<tokio::sync::Mutex<Vec<std::time::SystemTime>>>) -> Self {
        Self {
            failures_remaining: AtomicUsize::new(failures),
            attempt_times: times,
        }
    }
}

#[async_trait::async_trait]
impl S5Storage for TimingStorage {
    async fn put(&self, _path: &str, _data: Vec<u8>) -> Result<(), StorageError> {
        self.attempt_times.lock().await.push(std::time::SystemTime::now());
        
        if self.failures_remaining.fetch_sub(1, Ordering::SeqCst) > 0 {
            Err(StorageError::NetworkError("Simulated failure".into()))
        } else {
            Ok(())
        }
    }

    async fn get(&self, _path: &str) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(None)
    }

    async fn delete(&self, _path: &str) -> Result<(), StorageError> {
        Ok(())
    }

    async fn list(&self, _prefix: &str) -> Result<Vec<String>, StorageError> {
        Ok(vec![])
    }
}