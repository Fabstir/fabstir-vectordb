// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

pub mod encoder;
pub mod decoder;

pub use encoder::CborEncoder;
pub use decoder::CborDecoder;

// Re-export common types and errors
pub use serde_cbor::Error as CborError;
pub use serde_cbor::Value as CborValue;