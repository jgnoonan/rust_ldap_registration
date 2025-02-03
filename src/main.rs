/// Signal Registration Service
///
/// This is the main entry point for the Signal Registration Service implemented in Rust.
/// A gRPC service for user registration with Microsoft Entra ID authentication.
///
/// # Architecture
/// The service is built using:
/// - Microsoft Entra ID for user validation
///
/// # Flow
/// 1. Client sends registration request with username
/// 2. Service validates username with Microsoft Entra ID
/// 3. Service returns success response
///
/// @author Joseph G Noonan
/// @copyright 2025

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use thiserror::Error;
use tonic::transport::Server;
use tracing::{error, info};
use tracing_subscriber::{fmt, EnvFilter};

use entra_id_registration::auth::entra::{EntraIdClient, EntraIdConfig};
use entra_id_registration::config::Config;
use entra_id_registration::proto::registration_service_server::RegistrationServiceServer;
use entra_id_registration::grpc::RegistrationServer;

/// Service initialization errors
#[derive(Debug, Error)]
pub enum ServiceError {
    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),
    /// Logging initialization error
    #[error("Logging initialization error: {0}")]
    Logging(String),
    /// Server initialization error
    #[error("Server initialization error: {0}")]
    Server(#[from] tonic::transport::Error),
    /// Other error
    #[error("Other error: {0}")]
    Other(#[from] Box<dyn Error>),
}

/// Result type for service operations
type Result<T> = std::result::Result<T, ServiceError>;

/// Initializes the logging system with appropriate configuration.
fn init_logging(config: &Config) -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| {
            if config.application.name.contains("dev") {
                EnvFilter::try_new("debug")
            } else {
                EnvFilter::try_new("info")
            }
        })
        .unwrap_or_else(|_| EnvFilter::new("info"));

    fmt()
        .with_env_filter(env_filter)
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .with_thread_names(true)
        .with_level(true)
        .json()
        .try_init()
        .map_err(|e| ServiceError::Logging(e.to_string()))?;

    info!(
        app_name = %config.application.name,
        "📝 Logging initialized successfully"
    );
    Ok(())
}

/// Initializes and validates the configuration
fn validate_config(config: &Config) -> Result<()> {
    info!("🔍 Validating configuration...");

    // Validate Entra ID configuration
    let entra_config = &config.registration().directory.entra_id;
    if entra_config.tenant_id.is_empty() {
        error!("Missing Entra ID tenant ID");
        return Err(ServiceError::Config("Missing Entra ID tenant ID".into()));
    }
    if entra_config.client_id.is_empty() {
        error!("Missing Entra ID client ID");
        return Err(ServiceError::Config("Missing Entra ID client ID".into()));
    }
    if entra_config.client_secret.is_empty() {
        error!("Missing Entra ID client secret");
        return Err(ServiceError::Config("Missing Entra ID client secret".into()));
    }

    info!(
        tenant_id = %entra_config.tenant_id,
        client_id = %entra_config.client_id,
        "✅ Configuration validation successful"
    );
    Ok(())
}

/// Initializes all service dependencies
async fn init_service(config: Config) -> Result<()> {
    info!("🚀 Initializing registration service...");
    
    validate_config(&config)?;
    let registration_config = config.registration();

    // Initialize Entra ID client
    info!("🔑 Initializing Microsoft Entra ID client...");
    let entra_client = EntraIdClient::new(EntraIdConfig {
        tenant_id: registration_config.directory.entra_id.tenant_id.clone(),
        client_id: registration_config.directory.entra_id.client_id.clone(),
        client_secret: registration_config.directory.entra_id.client_secret.clone(),
        phone_number_attribute: registration_config.directory.entra_id.phone_number_attribute.clone(),
    }).map_err(|e| ServiceError::Config(e.to_string()))?;
    info!("✅ Microsoft Entra ID client initialized successfully");

    // Configure gRPC server
    let addr = format!(
        "{}:{}",
        registration_config.grpc.server.endpoint,
        registration_config.grpc.server.port
    )
    .parse()
    .map_err(|e| ServiceError::Config(format!("Invalid server address: {}", e)))?;

    info!(
        endpoint = %registration_config.grpc.server.endpoint,
        port = %registration_config.grpc.server.port,
        "📡 Starting gRPC server"
    );

    // Create registration server with session timeout
    let session_timeout = Duration::from_secs(registration_config.session_timeout_secs);
    let registration_server = RegistrationServer::new(Arc::new(entra_client))
        .with_session_timeout(session_timeout);

    info!(
        timeout_secs = registration_config.session_timeout_secs,
        "⏱️ Session timeout configured"
    );

    // Start the server
    Server::builder()
        .add_service(RegistrationServiceServer::new(registration_server))
        .serve(addr)
        .await?;

    Ok(())
}

/// Main function that:
/// 1. Loads configuration
/// 2. Sets up logging
/// 3. Initializes service dependencies
/// 4. Starts gRPC server
#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration first
    let config = Config::new().map_err(|e| ServiceError::Config(e.to_string()))?;
    
    // Initialize logging with config
    init_logging(&config)?;
    
    info!(
        app_name = %config.application.name,
        "🎉 Starting Signal Registration Service"
    );

    // Initialize and run service
    match init_service(config).await {
        Ok(()) => {
            info!("👋 Service shutdown gracefully");
            Ok(())
        }
        Err(e) => {
            error!(error = %e, "❌ Service failed");
            Err(e)
        }
    }
}
