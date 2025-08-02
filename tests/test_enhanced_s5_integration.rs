// tests/test_enhanced_s5_integration.rs

use vector_db::storage::{Storage, S5StorageAdapter, StorageMode, S5StorageConfig};
use vector_db::storage::{EnhancedS5Storage, S5StorageFactory};
use vector_db::core::{Vector, VectorId, Embedding};
use vector_db::types::VideoNFTMetadata;

mod phase_8_1_enhanced_s5_integration {
    use super::*;

    async fn is_mock_server_available() -> bool {
        match reqwest::get("http://localhost:5524/health").await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }

    mod s5_dependency_tests {
        use super::*;

        #[tokio::test]
        async fn should_create_s5_client_with_mock_configuration() {
            // Test that we can create an S5 client configured for mock server
            let config = S5StorageConfig {
                mode: StorageMode::Mock,
                mock_server_url: Some("http://localhost:5524".to_string()),
                portal_url: None,
                seed_phrase: None,
                connection_timeout: Some(5000),
                retry_attempts: Some(3),
            };

            let storage = EnhancedS5Storage::new(config);
            assert!(storage.is_ok(), "Should create storage instance");
        }

        #[tokio::test]
        async fn should_handle_s5_connection_errors_gracefully() {
            // Test with invalid server URL
            let config = S5StorageConfig {
                mode: StorageMode::Mock,
                mock_server_url: Some("http://invalid:99999".to_string()),
                portal_url: None,
                seed_phrase: None,
                connection_timeout: Some(1000),
                retry_attempts: Some(1),
            };

            let storage = EnhancedS5Storage::new(config).unwrap();
            let result = storage.is_connected().await;
            assert!(!result, "Should handle connection failure gracefully");
        }
    }

    mod s5_adapter_pattern_tests {
        use super::*;

        #[test]
        fn should_support_both_mock_and_real_storage_modes() {
            let mock_config = S5StorageConfig {
                mode: StorageMode::Mock,
                mock_server_url: Some("http://localhost:5524".to_string()),
                portal_url: None,
                seed_phrase: None,
                connection_timeout: None,
                retry_attempts: None,
            };

            let real_config = S5StorageConfig {
                mode: StorageMode::Real,
                mock_server_url: None,
                portal_url: Some("https://s5.vup.cx".to_string()),
                seed_phrase: Some("test seed phrase twelve words here for testing only".to_string()),
                connection_timeout: None,
                retry_attempts: None,
            };

            let mock_storage = EnhancedS5Storage::new(mock_config).unwrap();
            assert_eq!(mock_storage.get_mode(), StorageMode::Mock);

            let real_storage = EnhancedS5Storage::new(real_config).unwrap();
            assert_eq!(real_storage.get_mode(), StorageMode::Real);
        }

        #[test]
        fn should_validate_configuration_based_on_mode() {
            // Mock mode should require mock_server_url
            let invalid_mock_config = S5StorageConfig {
                mode: StorageMode::Mock,
                mock_server_url: None, // Missing required field
                portal_url: None,
                seed_phrase: None,
                connection_timeout: None,
                retry_attempts: None,
            };

            let result = EnhancedS5Storage::new(invalid_mock_config);
            assert!(result.is_err(), "Should fail without mock_server_url");
            assert!(result.unwrap_err().to_string().contains("mock_server_url"));

            // Real mode should require portal_url
            let invalid_real_config = S5StorageConfig {
                mode: StorageMode::Real,
                mock_server_url: None,
                portal_url: None, // Missing required field
                seed_phrase: None,
                connection_timeout: None,
                retry_attempts: None,
            };

            let result = EnhancedS5Storage::new(invalid_real_config);
            assert!(result.is_err(), "Should fail without portal_url");
            assert!(result.unwrap_err().to_string().contains("portal_url"));
        }

        #[tokio::test]
        async fn should_implement_s5_storage_adapter_trait() {
            let config = S5StorageConfig {
                mode: StorageMode::Mock,
                mock_server_url: Some("http://localhost:5524".to_string()),
                portal_url: None,
                seed_phrase: None,
                connection_timeout: None,
                retry_attempts: None,
            };

            let storage = EnhancedS5Storage::new(config).unwrap();
            
            // Test all trait methods are implemented
            assert_eq!(storage.get_mode(), StorageMode::Mock);
            
            // These should compile and run (even if mock server is not available)
            let _ = storage.is_connected().await;
            let _ = storage.get_stats().await;
        }
    }

    mod factory_pattern_tests {
        use super::*;
        use std::env;

        #[test]
        fn should_create_storage_instance_based_on_environment_variables() {
            // Test mock mode
            env::set_var("S5_MODE", "mock");
            env::set_var("S5_MOCK_SERVER_URL", "http://localhost:5524");
            
            let mock_storage = S5StorageFactory::create_from_env().unwrap();
            assert_eq!(mock_storage.get_mode(), StorageMode::Mock);

            // Test real mode
            env::set_var("S5_MODE", "real");
            env::set_var("S5_PORTAL_URL", "https://s5.vup.cx");
            env::set_var("S5_SEED_PHRASE", "test seed phrase for testing");
            
            let real_storage = S5StorageFactory::create_from_env().unwrap();
            assert_eq!(real_storage.get_mode(), StorageMode::Real);

            // Clean up
            env::remove_var("S5_MODE");
            env::remove_var("S5_MOCK_SERVER_URL");
            env::remove_var("S5_PORTAL_URL");
            env::remove_var("S5_SEED_PHRASE");
        }

        #[test]
        fn should_default_to_mock_mode_when_s5_mode_not_set() {
            env::remove_var("S5_MODE");
            env::set_var("S5_MOCK_SERVER_URL", "http://localhost:5524");
            
            let storage = S5StorageFactory::create_from_env().unwrap();
            assert_eq!(storage.get_mode(), StorageMode::Mock);

            env::remove_var("S5_MOCK_SERVER_URL");
        }

        #[test]
        fn should_support_custom_configuration_override() {
            let config = S5StorageConfig {
                mode: StorageMode::Real,
                mock_server_url: None,
                portal_url: Some("https://custom.portal.com".to_string()),
                seed_phrase: Some("custom seed phrase here".to_string()),
                connection_timeout: Some(15000),
                retry_attempts: Some(5),
            };

            let storage = S5StorageFactory::create(config).unwrap();
            assert_eq!(storage.get_mode(), StorageMode::Real);
        }
    }

    mod backward_compatibility_tests {
        use super::*;

        #[tokio::test]
        async fn should_maintain_compatibility_with_existing_storage_trait() {
            if !is_mock_server_available().await {
                eprintln!("Skipping test: Mock server not available at http://localhost:5524");
                return;
            }

            let config = S5StorageConfig {
                mode: StorageMode::Mock,
                mock_server_url: Some("http://localhost:5524".to_string()),
                portal_url: None,
                seed_phrase: None,
                connection_timeout: None,
                retry_attempts: None,
            };

            let storage = EnhancedS5Storage::new(config).unwrap();

            // Test vector operations using Storage trait
            let vector_id = VectorId::from_string("test-vector-001");
            let embedding = Embedding::new(vec![0.1, 0.2, 0.3]).unwrap();
            let vector = Vector::new(vector_id.clone(), embedding);

            // These should work with the existing Storage trait interface
            <EnhancedS5Storage as Storage>::put(&storage, &format!("vectors/{}", vector_id.to_string()), &vector).await.unwrap();
            
            let retrieved: Vector = <EnhancedS5Storage as Storage>::get(&storage, &format!("vectors/{}", vector_id.to_string())).await.unwrap();
            assert_eq!(retrieved.id, vector_id);

            let exists = <EnhancedS5Storage as Storage>::exists(&storage, &format!("vectors/{}", vector_id.to_string())).await.unwrap();
            assert!(exists);

            <EnhancedS5Storage as Storage>::delete(&storage, &format!("vectors/{}", vector_id.to_string())).await.unwrap();
            let after_delete = <EnhancedS5Storage as Storage>::exists(&storage, &format!("vectors/{}", vector_id.to_string())).await.unwrap();
            assert!(!after_delete);
        }

        #[tokio::test]
        async fn should_handle_batch_operations_efficiently() {
            if !is_mock_server_available().await {
                eprintln!("Skipping test: Mock server not available at http://localhost:5524");
                return;
            }

            let config = S5StorageConfig {
                mode: StorageMode::Mock,
                mock_server_url: Some("http://localhost:5524".to_string()),
                portal_url: None,
                seed_phrase: None,
                connection_timeout: None,
                retry_attempts: None,
            };

            let storage = EnhancedS5Storage::new(config).unwrap();

            // Create batch of vectors
            let vectors: Vec<Vector> = (0..10)
                .map(|i| {
                    let id = VectorId::from_string(&format!("batch-vector-{}", i));
                    let embedding = Embedding::new(vec![i as f32 * 0.1, i as f32 * 0.2, i as f32 * 0.3]).unwrap();
                    Vector::new(id, embedding)
                })
                .collect();

            // Batch put
            let put_futures: Vec<_> = vectors.iter()
                .map(|v| {
                    let storage = storage.clone();
                    let key = format!("vectors/{}", v.id.to_string());
                    let vector = v.clone();
                    async move {
                        <EnhancedS5Storage as Storage>::put(&storage, &key, &vector).await
                    }
                })
                .collect();
            
            futures::future::try_join_all(put_futures).await.unwrap();

            // Verify all exist
            let exists_futures: Vec<_> = vectors.iter()
                .map(|v| {
                    let storage = storage.clone();
                    let key = format!("vectors/{}", v.id.to_string());
                    async move {
                        <EnhancedS5Storage as Storage>::exists(&storage, &key).await
                    }
                })
                .collect();
            
            let exists_results = futures::future::try_join_all(exists_futures).await.unwrap();
            assert!(exists_results.iter().all(|&e| e));
        }

        #[tokio::test]
        async fn should_serialize_data_with_cbor_as_in_phase_7() {
            if !is_mock_server_available().await {
                eprintln!("Skipping test: Mock server not available at http://localhost:5524");
                return;
            }

            let config = S5StorageConfig {
                mode: StorageMode::Mock,
                mock_server_url: Some("http://localhost:5524".to_string()),
                portal_url: None,
                seed_phrase: None,
                connection_timeout: None,
                retry_attempts: None,
            };

            let storage = EnhancedS5Storage::new(config).unwrap();

            let metadata = VideoNFTMetadata {
                address: "0x123".to_string(),
                id: "nft-001".to_string(),
                name: "Test Video".to_string(),
                image: "https://example.com/image.jpg".to_string(),
                animation_url: Some("https://example.com/video.mp4".to_string()),
                mint_date_time: chrono::Utc::now(),
                genre: vec!["AI".to_string(), "Tutorial".to_string()],
                r#type: "video".to_string(),
                attributes: vec![],
                description: Some("Test video description".to_string()),
                poster_image: None,
                summary: None,
                supply: None,
                symbol: None,
                uri: None,
                user_pub: None,
                video: None,
            };

            <EnhancedS5Storage as Storage>::put(&storage, "metadata/test", &metadata).await.unwrap();
            let retrieved: VideoNFTMetadata = <EnhancedS5Storage as Storage>::get(&storage, "metadata/test").await.unwrap();
            
            // Should maintain CBOR serialization compatibility
            assert_eq!(retrieved.id, metadata.id);
            assert_eq!(retrieved.genre, vec!["AI", "Tutorial"]);
        }
    }
}