// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

use anyhow::Result;
use std::net::SocketAddr;
use tokio::signal;
use tracing::info;
use vector_db::api::rest::{create_app, ApiConfig};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "vector_db=info,tower_http=debug".into()),
        )
        .init();

    // Load configuration from environment
    let config = load_config();
    
    info!("Starting Vector Database server on {}:{}", config.host, config.port);

    // Create the application
    let app = create_app(config.clone()).await?;

    // Create the server address
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    
    // Create the server
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Server listening on {}", addr);

    // Run the server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("Server shutdown complete");
    Ok(())
}

fn load_config() -> ApiConfig {
    ApiConfig {
        host: std::env::var("VECTOR_DB_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
        port: std::env::var("VECTOR_DB_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8080),
        max_request_size: std::env::var("VECTOR_DB_MAX_REQUEST_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10 * 1024 * 1024), // 10MB default
        timeout: std::time::Duration::from_secs(
            std::env::var("VECTOR_DB_TIMEOUT_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30),
        ),
        cors_origins: std::env::var("VECTOR_DB_CORS_ORIGINS")
            .ok()
            .map(|origins| origins.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_else(|| vec!["http://localhost:3000".to_string()]),
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, starting graceful shutdown");
        }
        _ = terminate => {
            info!("Received terminate signal, starting graceful shutdown");
        }
    }
}