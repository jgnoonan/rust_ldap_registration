//! Signal Registration Service
//!
//! This is the main entry point for the Signal Registration Service implemented in Rust.
//! The service provides user registration functionality with LDAP authentication,
//! Twilio-based phone number verification, and DynamoDB storage integration.
//!
//! # Architecture
//! The service is built using:
//! - gRPC for API endpoints
//! - LDAP for user authentication
//! - Twilio for phone number verification
//! - DynamoDB for persistent storage
//!
//! # Flow
//! 1. User initiates registration with username
//! 2. Service validates username against LDAP
//! 3. User submits phone number for verification
//! 4. Service sends verification code via Twilio
//! 5. User submits verification code
//! 6. Service stores verified registration in DynamoDB
//!
//! @author Joseph G Noonan
//! @copyright 2025

use tonic::transport::Server;
use tracing::{info, Level};
use tracing_subscriber::fmt;
use rust_ldap_registration::proto::registration::registration_service_server::RegistrationServiceServer;
use rust_ldap_registration::grpc::RegistrationServer;
use rust_ldap_registration::ldap_validation::{LdapValidationServer, LdapValidationServiceServer};
use rust_ldap_registration::auth::ldap::{LdapClient, LdapConfig};
use rust_ldap_registration::db::dynamodb::DynamoDbClient;
use rust_ldap_registration::twilio::{TwilioClient, TwilioConfig};
use rust_ldap_registration::config::Config;
use rust_ldap_registration::twilio::rate_limit::{RateLimiter, RateLimitConfig};

/// Initializes the logging system with appropriate configuration.
///
/// Sets up structured logging with timestamps and log levels using
/// the tracing framework. Log level is configurable via environment.
///
/// # Returns
/// * `Result<()>` - Success or error if logging setup fails
fn setup_logging() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    fmt()
        .with_max_level(Level::DEBUG)
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .with_target(false)
        .with_level(true)
        .with_ansi(true)
        .with_writer(std::io::stdout)
        .try_init()
        .map_err(|e| e.into())
}

/// Initializes and starts all service dependencies.
///
/// Sets up the following components:
/// - LDAP client for authentication
/// - Twilio client for verification
/// - DynamoDB client for storage
/// - Rate limiter for request throttling
/// - gRPC server with registration endpoints
///
/// # Arguments
/// * `config` - Application configuration
///
/// # Returns
/// * `Result<()>` - Success or error if any service fails to start
async fn setup_services(config: Config) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let registration_config = config.registration();

    // Initialize LDAP client
    info!("Initializing LDAP client with URL: {}", registration_config.ldap.url);
    let ldap_config = LdapConfig {
        url: registration_config.ldap.url.clone(),
        bind_dn: registration_config.ldap.bind_dn.clone(),
        bind_password: registration_config.ldap.bind_password.clone(),
        base_dn: registration_config.ldap.base_dn.clone(),
        username_attribute: registration_config.ldap.username_attribute.clone(),
        phone_number_attribute: registration_config.ldap.phone_number_attribute.clone(),
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
        test_mode: registration_config.twilio.enabled,  // Use enabled flag as test mode indicator
    };
    let twilio_client = TwilioClient::new(twilio_config)?;
    info!("Twilio client initialized successfully");

    // Initialize rate limiter
    info!("Initializing rate limiter...");
    let rate_limiter = RateLimiter::new(RateLimitConfig::from(registration_config.rate_limits.clone()));
    info!("Rate limiter initialized successfully");

    let addr = format!("{}:{}", config.registration().grpc.server.endpoint, config.registration().grpc.server.port).parse()?;
    info!("Starting server on {}", addr);

    let ldap_service = LdapValidationServer::new(ldap_client.clone());

    let registration_server = RegistrationServer::new(
        ldap_client,
        twilio_client,
        dynamodb_client,
        rate_limiter,
        config.registration().grpc.timeout_secs,
    );

    Server::builder()
        .add_service(RegistrationServiceServer::new(registration_server))
        .add_service(LdapValidationServiceServer::new(ldap_service))
        .serve(addr)
        .await?;

    Ok(())
}

/// Main entry point for the registration service.
///
/// # Flow
/// 1. Initializes logging and configuration
/// 2. Sets up service dependencies (LDAP, Twilio, DynamoDB)
/// 3. Starts the gRPC server
///
/// # Returns
/// * `Result<()>` - Success or error if service fails to start
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    setup_logging()?;
    info!("Signal Registration Service starting up...");

    // Load configuration
    info!("Loading configuration...");
    let config = Config::new().map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
    info!("Configuration loaded successfully");

    setup_services(config).await?;

    Ok(())
}
