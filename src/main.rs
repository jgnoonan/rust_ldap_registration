/// Signal Registration Service
///
/// This is the main entry point for the Signal Registration Service implemented in Rust.
/// The service provides user registration functionality with LDAP authentication,
/// Twilio-based phone number verification, and DynamoDB storage integration.
///
/// # Copyright
/// Copyright (c) 2025 Signal Messenger, LLC
/// All rights reserved.
///
/// # License
/// Licensed under the AGPLv3 license.
/// Please see the LICENSE file in the root directory for details.

use tonic::transport::Server;
use tracing::{info, error, Level};
use tracing_subscriber::FmtSubscriber;
use rust_ldap_registration::proto::registration::registration_service_server::RegistrationServiceServer;
use rust_ldap_registration::grpc::RegistrationServer;
use rust_ldap_registration::auth::ldap::{LdapClient, LdapConfig};
use rust_ldap_registration::db::dynamodb::DynamoDbClient;
use rust_ldap_registration::twilio::{TwilioClient, TwilioConfig};
use rust_ldap_registration::config::Config;
use rust_ldap_registration::twilio::rate_limit::{RateLimiter, RateLimitConfig};

/// Main entry point for the registration service.
/// 
/// # Flow
/// 1. Initializes logging and configuration
/// 2. Sets up service dependencies (LDAP, Twilio, DynamoDB)
/// 3. Starts the gRPC server
///
/// # Errors
/// Returns an error if initialization or server startup fails
///
/// # Examples
/// ```no_run
/// use registration_service::main;
/// 
/// #[tokio::main]
/// async fn main() {
///     if let Err(e) = main().await {
///         eprintln!("Error: {}", e);
///         std::process::exit(1);
///     }
/// }
/// ```
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .with_thread_names(true)
        .pretty()
        .init();

    info!("Signal Registration Service starting up...");
    
    // Load configuration
    info!("Loading configuration...");
    let config = Config::new()?;
    let registration_config = config.registration();
    info!("Configuration loaded successfully");
    
    // Initialize LDAP client
    info!("Initializing LDAP client with URL: {}", registration_config.ldap.url);
    let ldap_config = LdapConfig {
        url: registration_config.ldap.url.clone(),
        bind_dn: registration_config.ldap.bind_dn.clone(),
        bind_password: registration_config.ldap.bind_password.clone(),
        search_base: registration_config.ldap.base_dn.clone(),
        search_filter: registration_config.ldap.user_filter.clone(),
        phone_number_attribute: registration_config.ldap.phone_number_attribute.clone(),
        connection_pool_size: registration_config.ldap.max_pool_size as usize,
        timeout_secs: 5,  // Set a shorter timeout
    };
    info!("Attempting to connect to LDAP server...");
    let ldap_client = LdapClient::new(ldap_config).await?;
    info!("LDAP client initialized successfully");
    
    // Initialize DynamoDB client
    info!("Initializing DynamoDB client with table: {}", registration_config.dynamodb.table_name);
    let dynamodb_client = DynamoDbClient::new(
        registration_config.dynamodb.table_name.clone(),
        registration_config.dynamodb.region.clone(),
    ).await?;
    info!("DynamoDB client initialized successfully");
    
    // Initialize Twilio client
    info!("Initializing Twilio client...");
    let twilio_config = TwilioConfig {
        account_sid: registration_config.twilio.account_sid.clone().expect("Twilio account SID is required"),
        auth_token: registration_config.twilio.auth_token.clone().expect("Twilio auth token is required"),
        verify_service_sid: registration_config.twilio.verify_service_sid.clone().expect("Twilio verify service SID is required"),
        verification_timeout_secs: registration_config.twilio.verification_timeout_secs,
    };
    let twilio_client = TwilioClient::new(twilio_config)?;
    info!("Twilio client initialized successfully");
    
    // Initialize rate limiter
    info!("Initializing rate limiter...");
    let rate_limiter = RateLimiter::new(RateLimitConfig::from(registration_config.rate_limits.clone()));
    info!("Rate limiter initialized successfully");
    
    // Create gRPC server
    let addr = format!("{}:{}", registration_config.grpc.server.endpoint, registration_config.grpc.server.port).parse()?;
    info!("Creating gRPC server with address: {}", addr);
    let registration_server = RegistrationServer::new(
        ldap_client,
        twilio_client,
        dynamodb_client,
        rate_limiter,
        registration_config.grpc.timeout_secs,
    );
     
    info!("Starting gRPC server on http://{}", addr);
    Server::builder()
        .add_service(RegistrationServiceServer::new(registration_server))
        .serve(addr)
        .await
        .map_err(|e| {
            error!("Server error: {}", e);
            e
        })?;

    info!("Server shutdown complete");
    Ok(())
}
