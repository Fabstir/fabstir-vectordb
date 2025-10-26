use crate::core::storage::S5Storage;
use crate::core::types::VectorId;
use crate::hybrid::core::{HybridConfig, HybridIndex};
use crate::hnsw::persistence::{HNSWPersister, PersistenceError as HNSWPersistenceError};
use crate::ivf::persistence::{IVFPersister, PersistenceError as IVFPersistenceError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

const CURRENT_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Incompatible version: expected <= {expected}, found {found}")]
    IncompatibleVersion { expected: u32, found: u32 },

    #[error("Invalid data: {0}")]
    InvalidData(String),

    #[error("HNSW persistence error: {0}")]
    HNSWError(String),

    #[error("IVF persistence error: {0}")]
    IVFError(String),

    #[error("Missing component: {0}")]
    MissingComponent(String),
}

impl From<HNSWPersistenceError> for PersistenceError {
    fn from(err: HNSWPersistenceError) -> Self {
        PersistenceError::HNSWError(err.to_string())
    }
}

impl From<IVFPersistenceError> for PersistenceError {
    fn from(err: IVFPersistenceError) -> Self {
        PersistenceError::IVFError(err.to_string())
    }
}

/// Metadata for serialized HybridIndex
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridMetadata {
    pub version: u32,
    pub config: HybridConfig,
    pub recent_count: usize,
    pub historical_count: usize,
    pub total_vectors: usize,
    pub timestamp: DateTime<Utc>,
}

impl HybridMetadata {
    /// Create metadata from a HybridIndex
    pub fn from_index(index: &HybridIndex) -> Self {
        let stats = index.get_stats();
        Self {
            version: CURRENT_VERSION,
            config: index.config().clone(),
            recent_count: stats.recent_vectors,
            historical_count: stats.historical_vectors,
            total_vectors: stats.total_vectors,
            timestamp: Utc::now(),
        }
    }

    /// Serialize metadata to CBOR bytes
    pub fn to_cbor(&self) -> Result<Vec<u8>, PersistenceError> {
        serde_cbor::to_vec(self).map_err(|e| PersistenceError::Serialization(e.to_string()))
    }

    /// Deserialize metadata from CBOR bytes
    pub fn from_cbor(data: &[u8]) -> Result<Self, PersistenceError> {
        let metadata: Self = serde_cbor::from_slice(data)
            .map_err(|e| PersistenceError::Deserialization(e.to_string()))?;

        // Version compatibility check
        if metadata.version > CURRENT_VERSION {
            return Err(PersistenceError::IncompatibleVersion {
                expected: CURRENT_VERSION,
                found: metadata.version,
            });
        }

        Ok(metadata)
    }
}

/// Serializable wrapper for vector timestamps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableTimestamps {
    pub timestamps: HashMap<VectorId, DateTime<Utc>>,
}

impl SerializableTimestamps {
    pub fn new(timestamps: HashMap<VectorId, DateTime<Utc>>) -> Self {
        Self { timestamps }
    }

    pub fn to_cbor(&self) -> Result<Vec<u8>, PersistenceError> {
        serde_cbor::to_vec(self).map_err(|e| PersistenceError::Serialization(e.to_string()))
    }

    pub fn from_cbor(data: &[u8]) -> Result<Self, PersistenceError> {
        serde_cbor::from_slice(data)
            .map_err(|e| PersistenceError::Deserialization(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hybrid_metadata_cbor_roundtrip() {
        let metadata = HybridMetadata {
            version: CURRENT_VERSION,
            config: HybridConfig::default(),
            recent_count: 100,
            historical_count: 500,
            total_vectors: 600,
            timestamp: Utc::now(),
        };

        let cbor = metadata.to_cbor().expect("Failed to serialize");
        let deserialized = HybridMetadata::from_cbor(&cbor).expect("Failed to deserialize");

        assert_eq!(deserialized.version, metadata.version);
        assert_eq!(deserialized.recent_count, metadata.recent_count);
        assert_eq!(deserialized.historical_count, metadata.historical_count);
        assert_eq!(deserialized.total_vectors, metadata.total_vectors);
    }

    #[test]
    fn test_serializable_timestamps_cbor_roundtrip() {
        let mut timestamps = HashMap::new();
        timestamps.insert(
            VectorId::from_string("test1"),
            Utc::now(),
        );
        timestamps.insert(
            VectorId::from_string("test2"),
            Utc::now(),
        );

        let serializable = SerializableTimestamps::new(timestamps.clone());
        let cbor = serializable.to_cbor().expect("Failed to serialize");
        let deserialized = SerializableTimestamps::from_cbor(&cbor).expect("Failed to deserialize");

        assert_eq!(deserialized.timestamps.len(), timestamps.len());
        for (id, _timestamp) in &timestamps {
            assert!(deserialized.timestamps.contains_key(id));
        }
    }

    #[test]
    fn test_version_compatibility() {
        let mut metadata = HybridMetadata {
            version: CURRENT_VERSION + 1, // Future version
            config: HybridConfig::default(),
            recent_count: 0,
            historical_count: 0,
            total_vectors: 0,
            timestamp: Utc::now(),
        };

        let cbor = serde_cbor::to_vec(&metadata).unwrap();
        let result = HybridMetadata::from_cbor(&cbor);

        assert!(result.is_err());
        match result {
            Err(PersistenceError::IncompatibleVersion { expected, found }) => {
                assert_eq!(expected, CURRENT_VERSION);
                assert_eq!(found, CURRENT_VERSION + 1);
            }
            _ => panic!("Expected IncompatibleVersion error"),
        }
    }
}
