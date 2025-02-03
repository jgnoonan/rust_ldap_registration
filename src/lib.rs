/// Signal Registration Service Library
///
/// This library provides functionality for user registration with Microsoft Entra ID authentication.
///
/// # Features
/// - Microsoft Entra ID authentication and user validation
/// - Twilio phone verification
/// - Rate limiting and session management
/// - gRPC service endpoints
///
/// # Modules
/// - `auth`: Authentication management
/// - `config`: Configuration management
/// - `grpc`: gRPC service implementation
/// - `proto`: Protocol buffer definitions
/// - `session`: Session management
/// - `twilio`: Phone verification and rate limiting
///
/// # Example
/// ```no_run
/// use entra_id_registration::{
///     twilio::TwilioClient,
///     grpc::RegistrationServer,
/// };
///
/// async fn setup_service() {
///     let twilio_client = TwilioClient::new(settings.twilio).expect("Failed to create Twilio client");
///     let rate_limiter = RateLimiter::new(settings.rate_limits);
///     
///     let server = RegistrationServer::new(
///         twilio_client,
///         rate_limiter,
///         settings.session_timeout_secs,
///     );
/// }
/// ```
///
/// @author Joseph G Noonan
/// @copyright 2025
/// Licensed under the AGPLv3 license.

pub mod auth;
pub mod config;
pub mod grpc;
pub mod session;
pub mod twilio;

/// Generated protocol buffer code
pub mod proto {
    tonic::include_proto!("org.signal.registration");
}

pub use auth::entra::EntraIdClient;
pub use grpc::RegistrationServer;