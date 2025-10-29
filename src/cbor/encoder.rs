// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

use anyhow::Result;
use serde::Serialize;
use serde_cbor::ser::Serializer;
use crate::types::{Vector, VideoNFTMetadata, S5Metadata};

pub struct CborEncoder;

impl CborEncoder {
    /// Encode a Vector with deterministic CBOR encoding
    pub fn encode_vector(vector: &Vector) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        let mut ser = Serializer::new(&mut buf);
        ser.self_describe()?;
        vector.serialize(&mut ser)?;
        Ok(buf)
    }

    /// Encode VideoNFTMetadata with deterministic CBOR encoding
    pub fn encode_metadata(metadata: &VideoNFTMetadata) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        let mut ser = Serializer::new(&mut buf);
        ser.self_describe()?;
        metadata.serialize(&mut ser)?;
        Ok(buf)
    }

    /// Encode S5-specific metadata
    pub fn encode_s5_metadata(metadata: &S5Metadata) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        let mut ser = Serializer::new(&mut buf);
        ser.self_describe()?;
        metadata.serialize(&mut ser)?;
        Ok(buf)
    }

    /// Encode a batch of vectors
    pub fn encode_batch(vectors: &[Vector]) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        let mut ser = Serializer::new(&mut buf);
        ser.self_describe()?;
        vectors.serialize(&mut ser)?;
        Ok(buf)
    }

    /// Encode with a CBOR tag
    pub fn encode_with_tag<T: Serialize>(value: &T, tag: u64) -> Result<Vec<u8>> {
        // Use serde_cbor::tags::Tagged to properly encode with a tag
        use serde_cbor::tags::Tagged;
        let tagged = Tagged::new(Some(tag), value);
        serde_cbor::to_vec(&tagged).map_err(Into::into)
    }

    /// Compress CBOR data using zstd
    pub fn compress(data: &[u8]) -> Result<Vec<u8>> {
        let compressed = zstd::encode_all(data, 3)?;
        Ok(compressed)
    }
}