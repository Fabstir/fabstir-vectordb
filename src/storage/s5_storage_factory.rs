// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

use std::env;
use std::error::Error;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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
        // Check STORAGE_MODE first, then fall back to S5_MODE
        let mode = match env::var("STORAGE_MODE").or_else(|_| env::var("S5_MODE")).as_deref() {
            Ok("real") => StorageMode::Real,
            Ok("mock") | _ => StorageMode::Mock, // Default to mock
        };

        let config = match mode {
            StorageMode::Mock => {
                let mock_server_url = env::var("S5_MOCK_SERVER_URL")
                    .unwrap_or_else(|_| "http://localhost:5522".to_string());
                
                // Validate URL format
                if !mock_server_url.starts_with("http://") && !mock_server_url.starts_with("https://") {
                    return Err(Box::new(StorageConfigError::new(
                        "Invalid URL format for S5_MOCK_SERVER_URL: must start with http:// or https://"
                    )));
                }
                
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
                    encrypt_at_rest: env::var("S5_ENCRYPT_AT_REST")
                        .ok()
                        .and_then(|v| v.parse().ok()),
                }
            }
            StorageMode::Real => {
                let portal_url = env::var("S5_PORTAL_URL")
                    .map_err(|_| StorageConfigError::new("S5_PORTAL_URL required for real mode"))?;
                
                // Validate URL format
                if !portal_url.starts_with("http://") && !portal_url.starts_with("https://") {
                    return Err(Box::new(StorageConfigError::new(
                        "Invalid URL format for S5_PORTAL_URL: must start with http:// or https://"
                    )));
                }
                
                // Get seed phrase from file or environment
                let seed_phrase = Self::load_seed_phrase()?;
                
                // Validate seed phrase if provided
                if let Some(ref phrase) = seed_phrase {
                    Self::validate_seed_phrase(phrase)?;
                }
                
                S5StorageConfig {
                    mode: StorageMode::Real,
                    mock_server_url: None,
                    portal_url: Some(portal_url),
                    seed_phrase,
                    connection_timeout: env::var("S5_CONNECTION_TIMEOUT")
                        .ok()
                        .and_then(|v| v.parse().ok()),
                    retry_attempts: env::var("S5_RETRY_ATTEMPTS")
                        .ok()
                        .and_then(|v| v.parse().ok()),
                    encrypt_at_rest: env::var("S5_ENCRYPT_AT_REST")
                        .ok()
                        .and_then(|v| v.parse().ok()),
                }
            }
        };

        // Log configuration summary (without sensitive data)
        Self::log_configuration_summary(&config);

        Self::create(config)
    }

    fn load_seed_phrase() -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
        // Check for seed phrase file first
        if let Ok(file_path) = env::var("S5_SEED_PHRASE_FILE") {
            #[cfg(unix)]
            {
                // Check file permissions on Unix systems
                if let Ok(metadata) = fs::metadata(&file_path) {
                    let permissions = metadata.permissions();
                    let mode = permissions.mode();
                    if mode & 0o077 != 0 {
                        eprintln!("WARNING: Seed phrase file '{}' has world-readable permissions. Consider running: chmod 600 {}", file_path, file_path);
                    }
                }
            }
            
            let seed_phrase = fs::read_to_string(&file_path)
                .map_err(|e| format!("Failed to read seed phrase file '{}': {}", file_path, e))?
                .trim()
                .to_string();
            
            if seed_phrase.is_empty() {
                return Err("Seed phrase file is empty".into());
            }
            
            return Ok(Some(seed_phrase));
        }
        
        // Fall back to environment variable
        Ok(env::var("S5_SEED_PHRASE").ok())
    }

    fn validate_seed_phrase(seed_phrase: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        let word_count = seed_phrase.split_whitespace().count();
        
        if word_count != 12 && word_count != 24 {
            return Err(Box::new(StorageConfigError::new(
                format!("Invalid seed phrase: expected 12 or 24 words, got {}", word_count)
            )));
        }
        
        Ok(())
    }

    fn log_configuration_summary(config: &S5StorageConfig) {
        eprintln!("S5 Storage Configuration:");
        eprintln!("  Mode: {:?}", config.mode);
        
        match config.mode {
            StorageMode::Mock => {
                if let Some(ref url) = config.mock_server_url {
                    eprintln!("  Mock Server URL: {}", url);
                }
            }
            StorageMode::Real => {
                if let Some(ref url) = config.portal_url {
                    eprintln!("  Portal URL: {}", url);
                }
                
                let seed_source = if env::var("S5_SEED_PHRASE_FILE").is_ok() {
                    "configured (from file)"
                } else if config.seed_phrase.is_some() {
                    "configured"
                } else {
                    "not configured"
                };
                eprintln!("  Seed phrase: {}", seed_source);
            }
        }
        
        if let Some(timeout) = config.connection_timeout {
            eprintln!("  Connection timeout: {}ms", timeout);
        }
        
        if let Some(attempts) = config.retry_attempts {
            eprintln!("  Retry attempts: {}", attempts);
        }
    }
}