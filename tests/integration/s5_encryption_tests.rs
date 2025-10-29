/// Integration tests for S5 encryption configuration
use vector_db::storage::s5_adapter::{S5StorageAdapter, S5StorageConfig, StorageMode};
use vector_db::storage::enhanced_s5_storage::EnhancedS5Storage;

// ============================================================================
// Encryption Default Tests
// ============================================================================

#[tokio::test]
async fn test_encryption_enabled_by_default() {
    // Create config without specifying encryption (should default to true)
    let config = S5StorageConfig {
        mode: StorageMode::Mock,
        mock_server_url: Some("http://localhost:5000".to_string()),
        portal_url: None,
        seed_phrase: None,
        connection_timeout: Some(5000),
        retry_attempts: Some(3),
        encrypt_at_rest: None, // Not specified - should default to true
    };

    let storage = EnhancedS5Storage::new(config).expect("Failed to create storage");

    // Verify encryption is enabled in stats
    let stats = storage.get_stats().await.expect("Failed to get stats");
    assert_eq!(stats["encryption_enabled"], true, "Encryption should be enabled by default");
}

#[tokio::test]
async fn test_explicit_encryption_enable() {
    // Explicitly enable encryption
    let config = S5StorageConfig {
        mode: StorageMode::Mock,
        mock_server_url: Some("http://localhost:5000".to_string()),
        portal_url: None,
        seed_phrase: None,
        connection_timeout: Some(5000),
        retry_attempts: Some(3),
        encrypt_at_rest: Some(true),
    };

    let storage = EnhancedS5Storage::new(config).expect("Failed to create storage");

    // Verify encryption is enabled in stats
    let stats = storage.get_stats().await.expect("Failed to get stats");
    assert_eq!(stats["encryption_enabled"], true, "Encryption should be explicitly enabled");
}

// ============================================================================
// Encryption Disable Tests
// ============================================================================

#[tokio::test]
async fn test_explicit_encryption_disable() {
    // Explicitly disable encryption
    let config = S5StorageConfig {
        mode: StorageMode::Mock,
        mock_server_url: Some("http://localhost:5000".to_string()),
        portal_url: None,
        seed_phrase: None,
        connection_timeout: Some(5000),
        retry_attempts: Some(3),
        encrypt_at_rest: Some(false),
    };

    let storage = EnhancedS5Storage::new(config).expect("Failed to create storage");

    // Verify encryption is disabled in stats
    let stats = storage.get_stats().await.expect("Failed to get stats");
    assert_eq!(stats["encryption_enabled"], false, "Encryption should be explicitly disabled");
}

// ============================================================================
// Encryption Header Tests
// ============================================================================

#[tokio::test]
async fn test_encryption_header_included_when_enabled() {
    // Note: This test verifies that encryption headers are configured
    // The actual header verification requires network inspection or mocking

    let config = S5StorageConfig {
        mode: StorageMode::Mock,
        mock_server_url: Some("http://localhost:5000".to_string()),
        portal_url: None,
        seed_phrase: None,
        connection_timeout: Some(5000),
        retry_attempts: Some(3),
        encrypt_at_rest: Some(true),
    };

    let storage = EnhancedS5Storage::new(config).expect("Failed to create storage");

    // Verify encryption is configured
    let stats = storage.get_stats().await.expect("Failed to get stats");
    assert_eq!(stats["encryption_enabled"], true);
    assert_eq!(stats["encryption_algorithm"], "xchacha20-poly1305");
}

#[tokio::test]
async fn test_encryption_header_not_included_when_disabled() {
    let config = S5StorageConfig {
        mode: StorageMode::Mock,
        mock_server_url: Some("http://localhost:5000".to_string()),
        portal_url: None,
        seed_phrase: None,
        connection_timeout: Some(5000),
        retry_attempts: Some(3),
        encrypt_at_rest: Some(false),
    };

    let storage = EnhancedS5Storage::new(config).expect("Failed to create storage");

    // Verify encryption is disabled
    let stats = storage.get_stats().await.expect("Failed to get stats");
    assert_eq!(stats["encryption_enabled"], false);
    // encryption_algorithm should not be present when disabled
    assert!(stats.get("encryption_algorithm").is_none() ||
            stats["encryption_algorithm"] == serde_json::Value::Null,
            "Encryption algorithm should not be set when encryption is disabled");
}

// ============================================================================
// Mode-Specific Tests
// ============================================================================

#[tokio::test]
async fn test_encryption_with_mock_mode() {
    let config = S5StorageConfig {
        mode: StorageMode::Mock,
        mock_server_url: Some("http://localhost:5000".to_string()),
        portal_url: None,
        seed_phrase: None,
        connection_timeout: Some(5000),
        retry_attempts: Some(3),
        encrypt_at_rest: Some(true),
    };

    let storage = EnhancedS5Storage::new(config).expect("Failed to create storage");

    let stats = storage.get_stats().await.expect("Failed to get stats");
    assert_eq!(stats["mode"], "Mock");
    assert_eq!(stats["encryption_enabled"], true);
}

#[tokio::test]
async fn test_encryption_with_real_mode() {
    let config = S5StorageConfig {
        mode: StorageMode::Real,
        mock_server_url: None,
        portal_url: Some("http://localhost:5522".to_string()),
        seed_phrase: Some("test-seed-phrase".to_string()),
        connection_timeout: Some(30000),
        retry_attempts: Some(3),
        encrypt_at_rest: Some(true),
    };

    let storage = EnhancedS5Storage::new(config).expect("Failed to create storage");

    let stats = storage.get_stats().await.expect("Failed to get stats");
    assert_eq!(stats["mode"], "Real");
    assert_eq!(stats["encryption_enabled"], true);
}

// ============================================================================
// Decryption Tests (S5.js handles this transparently)
// ============================================================================

#[tokio::test]
async fn test_get_decrypts_transparently() {
    // When encryption is enabled, S5.js should decrypt automatically on GET
    // This test verifies the configuration is set up correctly

    let config = S5StorageConfig {
        mode: StorageMode::Mock,
        mock_server_url: Some("http://localhost:5000".to_string()),
        portal_url: None,
        seed_phrase: None,
        connection_timeout: Some(5000),
        retry_attempts: Some(3),
        encrypt_at_rest: Some(true),
    };

    let storage = EnhancedS5Storage::new(config).expect("Failed to create storage");

    // Verify stats show encryption is enabled
    // Actual decryption is handled by S5.js backend
    let stats = storage.get_stats().await.expect("Failed to get stats");
    assert_eq!(stats["encryption_enabled"], true,
        "Encryption should be enabled for transparent decryption");
}
