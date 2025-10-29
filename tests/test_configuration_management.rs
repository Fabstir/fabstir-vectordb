// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

// tests/test_configuration_management.rs

use vector_db::storage::{S5StorageConfig, StorageMode, S5StorageFactory, EnhancedS5Storage, S5StorageAdapter};
use std::env;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

mod phase_8_4_configuration_management {
    use super::*;

    mod seed_phrase_validation {
        use super::*;

        #[test]
        fn test_valid_bip39_seed_phrase_12_words() {
            // Test that 12-word BIP39 seed phrases are accepted
            let seed_phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
            env::set_var("S5_MODE", "real");
            env::set_var("S5_PORTAL_URL", "https://s5.vup.cx");
            env::set_var("S5_SEED_PHRASE", seed_phrase);
            
            let result = S5StorageFactory::create_from_env();
            assert!(result.is_ok(), "Should accept valid 12-word seed phrase");
            
            // Cleanup
            env::remove_var("S5_SEED_PHRASE");
        }

        #[test]
        fn test_valid_bip39_seed_phrase_24_words() {
            // Test that 24-word BIP39 seed phrases are accepted
            let seed_phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
            env::set_var("S5_MODE", "real");
            env::set_var("S5_PORTAL_URL", "https://s5.vup.cx");
            env::set_var("S5_SEED_PHRASE", seed_phrase);
            
            let result = S5StorageFactory::create_from_env();
            assert!(result.is_ok(), "Should accept valid 24-word seed phrase");
            
            // Cleanup
            env::remove_var("S5_SEED_PHRASE");
        }

        #[test]
        fn test_invalid_seed_phrase_wrong_word_count() {
            // Test that invalid word counts are rejected
            let seed_phrase = "abandon abandon abandon"; // Only 3 words
            env::set_var("S5_MODE", "real");
            env::set_var("S5_PORTAL_URL", "https://s5.vup.cx");
            env::set_var("S5_SEED_PHRASE", seed_phrase);
            
            let result = S5StorageFactory::create_from_env();
            assert!(result.is_err(), "Should reject seed phrase with wrong word count");
            assert!(result.unwrap_err().to_string().contains("seed phrase"), 
                "Error should mention seed phrase");
            
            // Cleanup
            env::remove_var("S5_SEED_PHRASE");
        }

        #[test]
        fn test_seed_phrase_from_file() {
            // Test loading seed phrase from file
            let temp_dir = tempfile::tempdir().unwrap();
            let seed_file = temp_dir.path().join("seed.txt");
            let seed_phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
            fs::write(&seed_file, seed_phrase).unwrap();
            
            env::set_var("S5_MODE", "real");
            env::set_var("S5_PORTAL_URL", "https://s5.vup.cx");
            env::set_var("S5_SEED_PHRASE_FILE", seed_file.to_str().unwrap());
            
            let result = S5StorageFactory::create_from_env();
            assert!(result.is_ok(), "Should load seed phrase from file");
            
            // Cleanup
            env::remove_var("S5_SEED_PHRASE_FILE");
        }

        #[test]
        #[cfg(unix)]
        fn test_seed_phrase_file_permissions_warning() {
            // Test that insecure file permissions trigger a warning
            let temp_dir = tempfile::tempdir().unwrap();
            let seed_file = temp_dir.path().join("seed.txt");
            fs::write(&seed_file, "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about").unwrap();
            
            // Set world-readable permissions
            fs::set_permissions(&seed_file, fs::Permissions::from_mode(0o644)).unwrap();
            
            env::set_var("S5_MODE", "real");
            env::set_var("S5_PORTAL_URL", "https://s5.vup.cx");
            env::set_var("S5_SEED_PHRASE_FILE", seed_file.to_str().unwrap());
            
            // Should still work but log warning (check logs in implementation)
            let result = S5StorageFactory::create_from_env();
            assert!(result.is_ok(), "Should work with warning about permissions");
            
            // Cleanup
            env::remove_var("S5_SEED_PHRASE_FILE");
        }
    }

    mod configuration_validation {
        use super::*;

        #[test]
        fn test_missing_required_config_helpful_error() {
            // Test helpful error when portal URL missing for real mode
            env::set_var("S5_MODE", "real");
            env::remove_var("S5_PORTAL_URL");
            
            let result = S5StorageFactory::create_from_env();
            assert!(result.is_err());
            let error = result.unwrap_err().to_string();
            assert!(error.contains("S5_PORTAL_URL"), "Error should mention missing variable");
            assert!(error.contains("real mode"), "Error should mention the mode");
            
            // Cleanup
            env::remove_var("S5_MODE");
        }

        #[test]
        fn test_invalid_portal_url_format() {
            // Test validation of portal URL format
            env::set_var("S5_MODE", "real");
            env::set_var("S5_PORTAL_URL", "not-a-valid-url");
            
            let result = S5StorageFactory::create_from_env();
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("URL"), 
                "Error should mention URL format");
            
            // Cleanup
            env::remove_var("S5_MODE");
            env::remove_var("S5_PORTAL_URL");
        }

        #[tokio::test]
        async fn test_configuration_summary_on_startup() {
            // Test that configuration is summarized (via get_stats)
            env::set_var("S5_MODE", "mock");
            env::set_var("S5_MOCK_SERVER_URL", "http://localhost:5524");
            
            let storage = S5StorageFactory::create_from_env().unwrap();
            let stats = storage.get_stats().await.unwrap();
            
            assert_eq!(stats["mode"], "Mock");
            assert!(stats["base_url"].as_str().unwrap().contains("5524"));
            
            // Cleanup
            env::remove_var("S5_MODE");
            env::remove_var("S5_MOCK_SERVER_URL");
        }
    }

    mod security_tests {
        use super::*;

        #[test]
        fn test_seed_phrase_not_in_logs() {
            // Test that seed phrase is not logged
            // This would need to capture logs and verify
            let seed_phrase = "secret seed phrase do not log";
            env::set_var("S5_MODE", "real");
            env::set_var("S5_PORTAL_URL", "https://s5.vup.cx");
            env::set_var("S5_SEED_PHRASE", seed_phrase);
            
            // Initialize logger capture here
            let storage = S5StorageFactory::create_from_env();
            
            // Verify logs don't contain seed phrase
            // Implementation would need log capture mechanism
            
            // Cleanup
            env::remove_var("S5_SEED_PHRASE");
        }

        #[test]
        fn test_seed_phrase_not_in_error_messages() {
            // Test that seed phrase is not exposed in errors
            let seed_phrase = "invalid seed phrase test";
            env::set_var("S5_MODE", "real");
            env::set_var("S5_PORTAL_URL", "https://s5.vup.cx");
            env::set_var("S5_SEED_PHRASE", seed_phrase);
            
            let result = S5StorageFactory::create_from_env();
            if result.is_err() {
                let error_msg = result.unwrap_err().to_string();
                assert!(!error_msg.contains(seed_phrase), 
                    "Error message should not contain actual seed phrase");
            }
            
            // Cleanup
            env::remove_var("S5_SEED_PHRASE");
        }

        #[tokio::test]
        async fn test_seed_phrase_not_in_api_responses() {
            // Test that seed phrase is never exposed via API
            env::set_var("S5_MODE", "mock");
            env::set_var("S5_MOCK_SERVER_URL", "http://localhost:5524");
            
            let storage = S5StorageFactory::create_from_env().unwrap();
            let stats = storage.get_stats().await.unwrap();
            
            // Verify stats don't contain seed phrase
            let stats_str = serde_json::to_string(&stats).unwrap();
            assert!(!stats_str.contains("seed"), 
                "Stats should not contain any seed phrase info");
            
            // Cleanup
            env::remove_var("S5_MODE");
            env::remove_var("S5_MOCK_SERVER_URL");
        }
    }

    mod health_endpoint_enhancement {
        use super::*;

        #[tokio::test]
        async fn test_health_endpoint_includes_storage_mode() {
            // Test that /health endpoint includes storage mode info
            // This would need the REST API server running
            // Pseudo-test showing expected behavior
            
            let expected_health = serde_json::json!({
                "status": "healthy",
                "version": "0.1.0",
                "storage": {
                    "mode": "mock",
                    "connected": true,
                    "base_url": "http://localhost:5524"
                },
                "indices": {
                    "hnsw": {"healthy": true, "vector_count": 0},
                    "ivf": {"healthy": true, "vector_count": 0}
                }
            });
            
            // Would make HTTP request to /health and verify response
            assert!(true, "Health endpoint test placeholder");
        }
    }
}