use anyhow::Result;
use serde::de::DeserializeOwned;
use crate::types::{Vector, VideoNFTMetadata, S5Metadata};

pub struct CborDecoder;

impl CborDecoder {
    /// Decode a Vector from CBOR bytes
    pub fn decode_vector(data: &[u8]) -> Result<Vector> {
        let vector = serde_cbor::from_slice(data)?;
        Ok(vector)
    }

    /// Decode VideoNFTMetadata from CBOR bytes
    pub fn decode_metadata(data: &[u8]) -> Result<VideoNFTMetadata> {
        let metadata = serde_cbor::from_slice(data)?;
        Ok(metadata)
    }

    /// Decode S5-specific metadata
    pub fn decode_s5_metadata(data: &[u8]) -> Result<S5Metadata> {
        let metadata = serde_cbor::from_slice(data)?;
        Ok(metadata)
    }

    /// Decode a batch of vectors
    pub fn decode_batch(data: &[u8]) -> Result<Vec<Vector>> {
        let vectors = serde_cbor::from_slice(data)?;
        Ok(vectors)
    }

    /// Decompress CBOR data using zstd
    pub fn decompress(data: &[u8]) -> Result<Vec<u8>> {
        let decompressed = zstd::decode_all(data)?;
        Ok(decompressed)
    }

    /// Decode any type that implements DeserializeOwned
    pub fn decode<T: DeserializeOwned>(data: &[u8]) -> Result<T> {
        let value = serde_cbor::from_slice(data)?;
        Ok(value)
    }
}