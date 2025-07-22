use vector_db::core::storage::*;
use vector_db::core::types::*;
use async_trait::async_trait;
use std::collections::HashMap;
use tokio;

#[cfg(test)]
mod s5_storage_tests {
    use super::*;

    // Mock S5 storage for testing
    struct MockS5Storage {
        data: tokio::sync::RwLock<HashMap<String, Vec<u8>>>,
        call_count: tokio::sync::RwLock<HashMap<String, usize>>,
    }

    impl MockS5Storage {
        fn new() -> Self {
            Self {
                data: tokio::sync::RwLock::new(HashMap::new()),
                call_count: tokio::sync::RwLock::new(HashMap::new()),
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
            Ok(data.keys()
                .filter(|k| k.starts_with(prefix))
                .cloned()
                .collect())
        }
    }

    #[tokio::test]
    async fn test_basic_storage_operations() {
        let storage = MockS5Storage::new();
        let path = "/test/data";
        let data = b"test data".to_vec();

        // Test put
        storage.put(path, data.clone()).await.unwrap();

        // Test get
        let retrieved = storage.get(path).await.unwrap();
        assert_eq!(retrieved, Some(data));

        // Test list
        let items = storage.list("/test").await.unwrap();
        assert_eq!(items.len(), 1);
        assert!(items.contains(&path.to_string()));

        // Test delete
        storage.delete(path).await.unwrap();
        let deleted = storage.get(path).await.unwrap();
        assert_eq!(deleted, None);
    }

    #[tokio::test]
    async fn test_cached_storage() {
        let base = MockS5Storage::new();
        let cached = CachedS5Storage::new(base, 100);
        
        let path = "/cached/test";
        let data = b"cached data".to_vec();

        // First put
        cached.put(path, data.clone()).await.unwrap();

        // First get (cache hit - put populated the cache)
        let result1 = cached.get(path).await.unwrap();
        assert_eq!(result1, Some(data.clone()));

        // Second get (cache hit)
        let result2 = cached.get(path).await.unwrap();
        assert_eq!(result2, Some(data));

        // Verify cache is working by checking call count
        let stats = cached.stats().await;
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 0);
    }

    #[tokio::test]
    async fn test_batch_operations() {
        let storage = MockS5Storage::new();
        let batch = BatchS5Storage::new(storage, 10);

        // Add multiple items
        for i in 0..5 {
            let path = format!("/batch/item_{}", i);
            let data = format!("data_{}", i).into_bytes();
            batch.put(&path, data).await.unwrap();
        }

        // Verify they're stored
        for i in 0..5 {
            let path = format!("/batch/item_{}", i);
            let result = batch.get(&path).await.unwrap();
            assert!(result.is_some());
        }
    }

    #[tokio::test]
    async fn test_storage_retry_logic() {
        // Create a flaky storage that fails first 2 attempts
        struct FlakyStorage {
            attempts: tokio::sync::RwLock<usize>,
            inner: MockS5Storage,
        }

        #[async_trait]
        impl S5Storage for FlakyStorage {
            async fn get(&self, path: &str) -> Result<Option<Vec<u8>>, StorageError> {
                let mut attempts = self.attempts.write().await;
                *attempts += 1;
                
                if *attempts < 3 {
                    Err(StorageError::NetworkError("Simulated failure".into()))
                } else {
                    self.inner.get(path).await
                }
            }

            async fn put(&self, path: &str, data: Vec<u8>) -> Result<(), StorageError> {
                self.inner.put(path, data).await
            }

            async fn delete(&self, path: &str) -> Result<(), StorageError> {
                self.inner.delete(path).await
            }

            async fn list(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
                self.inner.list(prefix).await
            }
        }

        let flaky = FlakyStorage {
            attempts: tokio::sync::RwLock::new(0),
            inner: MockS5Storage::new(),
        };
        
        let retry_storage = RetryS5Storage::new(flaky, 3);
        
        // This should succeed after retries
        retry_storage.put("/test", b"data".to_vec()).await.unwrap();
        let result = retry_storage.get("/test").await.unwrap();
        assert!(result.is_some());
    }
}