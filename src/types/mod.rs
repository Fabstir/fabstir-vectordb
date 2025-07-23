use serde::{Deserialize, Serialize};
use anyhow::Result;

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
    pub description: String,
    pub genre: Vec<String>,
    pub id: String,
    pub image: String,
    #[serde(rename = "mintDateTime")]
    pub mintDateTime: String,
    pub name: String,
    #[serde(rename = "posterImage", skip_serializing_if = "Option::is_none")]
    pub posterImage: Option<String>,
    pub summary: String,
    pub supply: u32,
    pub symbol: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub uri: String,
    #[serde(rename = "userPub")]
    pub userPub: String,
    pub video: String,
}

impl VideoNFTMetadata {
    pub fn to_cbor(&self) -> Result<Vec<u8>> {
        Ok(serde_cbor::to_vec(self)?)
    }
    
    pub fn from_cbor(data: &[u8]) -> Result<Self> {
        Ok(serde_cbor::from_slice(data)?)
    }
}