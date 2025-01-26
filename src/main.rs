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

use std::net::SocketAddr;
use tonic::transport::Server;
use anyhow::Result;

mod auth;
mod db;
mod twilio;
mod grpc;
mod config;

use auth::ldap::LdapClient;
use db::dynamodb::DynamoDbClient;
use twilio::{TwilioClient, rate_limit::RateLimiter};
use grpc::registration::registration_service_server::RegistrationServiceServer;
use grpc::RegistrationServer;
use config::Settings;

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
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    // Load configuration
    let settings = Settings::new()?;
    
    // Initialize clients
    let ldap_client = LdapClient::new(settings.ldap.into())?;
    
    let twilio_config = twilio::TwilioConfig {
        account_sid: settings.twilio.account_sid.unwrap_or_default(),
        auth_token: settings.twilio.auth_token.unwrap_or_default(),
        verify_service_sid: settings.twilio.verify_service_sid.unwrap_or_default(),
        verification_timeout_secs: settings.twilio.verification_timeout_secs,
    };
    let twilio_client = TwilioClient::new(twilio_config)?;
    
    let dynamodb_config = db::dynamodb::DynamoDbConfig {
        table_name: settings.dynamodb.table_name,
        region: settings.dynamodb.region,
        endpoint: settings.dynamodb.endpoint,
    };
    let dynamodb_client = DynamoDbClient::new(dynamodb_config).await?;
    
    let rate_limit_config = twilio::rate_limit::RateLimitConfig {
        max_attempts: settings.rate_limits.check_verification_code.delays as u32,
        window_secs: settings.rate_limits.leaky_bucket.session_creation.permit_regeneration_period,
    };
    let rate_limiter = RateLimiter::new(rate_limit_config);
    
    // Create gRPC server
    let addr: SocketAddr = settings.get_socket_addr()
        .parse()
        .unwrap_or_else(|_| "[::1]:50051".parse().unwrap());
        
    let registration_server = RegistrationServer::new(
        ldap_client,
        twilio_client,
        dynamodb_client,
        rate_limiter,
        settings.twilio.verification_timeout_secs as u64,
    );
    
    println!("Starting gRPC server on {}", addr);
    
    Server::builder()
        .add_service(RegistrationServiceServer::new(registration_server))
        .serve(addr)
        .await?;
        
    Ok(())
}
