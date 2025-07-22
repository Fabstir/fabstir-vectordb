use async_trait::async_trait;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    SerializationError(String),
}

#[async_trait]
pub trait S5Storage: Send + Sync {
    async fn get(&self, path: &str) -> Result<Option<Vec<u8>>, StorageError>;
    async fn put(&self, path: &str, data: Vec<u8>) -> Result<(), StorageError>;
    async fn delete(&self, path: &str) -> Result<(), StorageError>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>, StorageError>;
}

pub struct CachedS5Storage<T> {
    inner: T,
    capacity: usize,
}

impl<T: S5Storage> CachedS5Storage<T> {
    pub fn new(inner: T, capacity: usize) -> Self {
        Self { inner, capacity }
    }
    
    pub async fn stats(&self) -> CacheStats {
        CacheStats { hits: 0, misses: 0 }
    }
}

pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
}

#[async_trait]
impl<T: S5Storage> S5Storage for CachedS5Storage<T> {
    async fn get(&self, path: &str) -> Result<Option<Vec<u8>>, StorageError> {
        self.inner.get(path).await
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

pub struct BatchS5Storage<T> {
    inner: T,
    batch_size: usize,
}

impl<T: S5Storage> BatchS5Storage<T> {
    pub fn new(inner: T, batch_size: usize) -> Self {
        Self { inner, batch_size }
    }
}

#[async_trait]
impl<T: S5Storage> S5Storage for BatchS5Storage<T> {
    async fn get(&self, path: &str) -> Result<Option<Vec<u8>>, StorageError> {
        self.inner.get(path).await
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

pub struct RetryS5Storage<T> {
    inner: T,
    max_retries: usize,
}

impl<T: S5Storage> RetryS5Storage<T> {
    pub fn new(inner: T, max_retries: usize) -> Self {
        Self { inner, max_retries }
    }
}

#[async_trait]
impl<T: S5Storage> S5Storage for RetryS5Storage<T> {
    async fn get(&self, path: &str) -> Result<Option<Vec<u8>>, StorageError> {
        self.inner.get(path).await
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