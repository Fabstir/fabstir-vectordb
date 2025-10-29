// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

use serde::{Deserialize, Serialize};
use anyhow::Result;
use chrono;

// Vector type for tests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vector {
    pub id: String,
    pub values: Vec<f32>,
    pub metadata: Option<serde_json::Value>,
}

impl Vector {
    pub fn to_cbor(&self) -> Result<Vec<u8>> {
        Ok(serde_cbor::to_vec(self)?)
    }
    
    pub fn from_cbor(data: &[u8]) -> Result<Self> {
        Ok(serde_cbor::from_slice(data)?)
    }
}

// Video NFT metadata types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attribute {
    pub key: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VideoNFTMetadata {
    pub address: String,
    pub attributes: Vec<Attribute>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub genre: Vec<String>,
    pub id: String,
    pub image: String,
    #[serde(rename = "mint_date_time", alias = "mintDateTime")]
    pub mint_date_time: chrono::DateTime<chrono::Utc>,
    pub name: String,
    #[serde(rename = "poster_image", alias = "posterImage", skip_serializing_if = "Option::is_none")]
    pub poster_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supply: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(rename = "user_pub", alias = "userPub", skip_serializing_if = "Option::is_none")]
    pub user_pub: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<String>,
    #[serde(rename = "animation_url", skip_serializing_if = "Option::is_none")]
    pub animation_url: Option<String>,
}

impl VideoNFTMetadata {
    pub fn to_cbor(&self) -> Result<Vec<u8>> {
        Ok(serde_cbor::to_vec(self)?)
    }
    
    pub fn from_cbor(data: &[u8]) -> Result<Self> {
        Ok(serde_cbor::from_slice(data)?)
    }
}

// S5-specific metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S5Metadata {
    pub cid: String,
    pub size: usize,
    pub mime_type: String,
    pub created_at: i64, // Unix timestamp
    pub encryption: Option<String>,
}