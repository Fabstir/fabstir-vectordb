use std::env;
use std::error::Error;

use crate::storage::{
    enhanced_s5_storage::EnhancedS5Storage,
    s5_adapter::{S5StorageConfig, StorageMode, StorageConfigError},
};

pub struct S5StorageFactory;

impl S5StorageFactory {
    pub fn create(config: S5StorageConfig) -> Result<EnhancedS5Storage, Box<dyn Error + Send + Sync>> {
        EnhancedS5Storage::new(config)
    }

    pub fn create_from_env() -> Result<EnhancedS5Storage, Box<dyn Error + Send + Sync>> {
        let mode = match env::var("S5_MODE").as_deref() {
            Ok("real") => StorageMode::Real,
            Ok("mock") | _ => StorageMode::Mock, // Default to mock
        };

        let config = match mode {
            StorageMode::Mock => {
                let mock_server_url = env::var("S5_MOCK_SERVER_URL")
                    .map_err(|_| StorageConfigError::new("S5_MOCK_SERVER_URL not set for mock mode"))?;
                
                S5StorageConfig {
                    mode: StorageMode::Mock,
                    mock_server_url: Some(mock_server_url),
                    portal_url: None,
                    seed_phrase: None,
                    connection_timeout: env::var("S5_CONNECTION_TIMEOUT")
                        .ok()
                        .and_then(|v| v.parse().ok()),
                    retry_attempts: env::var("S5_RETRY_ATTEMPTS")
                        .ok()
                        .and_then(|v| v.parse().ok()),
                }
            }
            StorageMode::Real => {
                let portal_url = env::var("S5_PORTAL_URL")
                    .map_err(|_| StorageConfigError::new("S5_PORTAL_URL not set for real mode"))?;
                
                S5StorageConfig {
                    mode: StorageMode::Real,
                    mock_server_url: None,
                    portal_url: Some(portal_url),
                    seed_phrase: env::var("S5_SEED_PHRASE").ok(),
                    connection_timeout: env::var("S5_CONNECTION_TIMEOUT")
                        .ok()
                        .and_then(|v| v.parse().ok()),
                    retry_attempts: env::var("S5_RETRY_ATTEMPTS")
                        .ok()
                        .and_then(|v| v.parse().ok()),
                }
            }
        };

        Self::create(config)
    }
}