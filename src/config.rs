/// Configuration Module
///
/// Provides configuration management for the Signal Registration Service.
/// Handles loading and parsing of YAML configuration files and environment variables.
/// Supports multiple environments (development, production) and local overrides.
///
/// # Copyright
/// Copyright (c) 2025 Signal Messenger, LLC
/// All rights reserved.
///
/// # License
/// Licensed under the AGPLv3 license.
/// Please see the LICENSE file in the root directory for details.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use config::{Config as ConfigFile, File, Environment};

/// Application metadata configuration
#[derive(Debug, Deserialize, Serialize)]
pub struct Application {
    /// Name of the application
    pub name: String,
}

/// Metrics configuration
#[derive(Debug, Deserialize, Serialize)]
pub struct Metrics {
    /// Whether metrics collection is enabled
    pub enabled: bool,
    /// Metrics export configuration
    pub export: MetricsExport,
}

/// Metrics export configuration
#[derive(Debug, Deserialize, Serialize)]
pub struct MetricsExport {
    /// Datadog-specific configuration
    pub datadog: DatadogConfig,
}

/// Datadog configuration
#[derive(Debug, Deserialize, Serialize)]
pub struct DatadogConfig {
    /// Whether Datadog export is enabled
    pub enabled: bool,
}

/// Rate limiting configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RateLimits {
    /// Check verification code rate limits
    #[serde(rename = "check_verification_code")]
    pub check_verification_code: DelayConfig,
    /// Leaky bucket rate limits
    pub leaky_bucket: LeakyBucketConfig,
    /// SMS verification code rate limits
    #[serde(rename = "send_sms_verification_code")]
    pub send_sms_verification_code: DelayConfig,
    /// Voice verification code rate limits
    #[serde(rename = "send_voice_verification_code")]
    pub send_voice_verification_code: VoiceDelayConfig,
}

/// Delay configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DelayConfig {
    /// Delay in seconds
    pub delays: u64,
    /// Java-compatible delay string (ignored)
    #[serde(rename = "delays_seconds", skip_serializing_if = "Option::is_none")]
    pub delays_seconds: Option<String>,
}

/// Voice delay configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct VoiceDelayConfig {
    /// Delay in seconds
    pub delays: u64,
    /// Java-compatible delay string (ignored)
    #[serde(rename = "delays_seconds", skip_serializing_if = "Option::is_none")]
    pub delays_seconds: Option<String>,
    /// Maximum number of attempts
    pub max_attempts: u32,
    /// Delay after first SMS in seconds
    pub delay_after_first_sms: u64,
}

/// Session creation configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SessionCreationConfig {
    /// Name of the rate limit
    pub name: String,
    /// Maximum capacity
    pub max_capacity: u32,
    /// Leak rate
    pub leak_rate: f64,
    /// Initial number of tokens
    pub initial_tokens: u32,
    /// Permit regeneration period in seconds
    pub permit_regeneration_period: u64,
    /// Minimum delay in seconds
    pub min_delay: u64,
}

/// Leaky bucket configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LeakyBucketConfig {
    /// Session creation configuration
    pub session_creation: SessionCreationConfig,
}

/// Twilio configuration
#[derive(Debug, Deserialize, Serialize)]
pub struct TwilioConfig {
    /// Whether Twilio is enabled
    pub enabled: bool,
    /// Verification timeout in seconds
    pub verification_timeout_secs: u64,
    /// Twilio account SID
    pub account_sid: Option<String>,
    /// Twilio auth token
    pub auth_token: Option<String>,
    /// Twilio verify service SID
    pub verify_service_sid: Option<String>,
}

/// gRPC server configuration
#[derive(Debug, Deserialize, Serialize)]
pub struct GrpcConfig {
    /// Server configuration
    pub server: ServerConfig,
    /// Session timeout in seconds
    pub timeout_secs: u64,
}

/// Server configuration
#[derive(Debug, Deserialize, Serialize)]
pub struct ServerConfig {
    /// Server endpoint
    pub endpoint: String,
    /// Server port
    pub port: u16,
    /// Operation timeout in seconds
    pub timeout_secs: u64,
}

/// Directory configuration
#[derive(Debug, Deserialize, Serialize)]
pub struct DirectoryConfig {
    /// Directory type
    pub r#type: String,
    /// Microsoft Entra ID configuration
    pub entra_id: EntraIdConfig,
}

/// Microsoft Entra ID configuration
#[derive(Debug, Deserialize, Serialize)]
pub struct EntraIdConfig {
    /// Tenant ID
    pub tenant_id: String,
    /// Client ID
    pub client_id: String,
    /// Client secret
    pub client_secret: String,
    /// Phone number attribute
    pub phone_number_attribute: String,
}

/// Registration configuration
#[derive(Debug, Deserialize, Serialize)]
pub struct RegistrationConfig {
    /// Whether to use LDAP
    pub use_ldap: bool,
    /// Session timeout in seconds
    pub session_timeout_secs: u64,
    /// gRPC configuration
    pub grpc: GrpcConfig,
    /// Directory configuration
    pub directory: DirectoryConfig,
    /// Twilio configuration (optional)
    pub twilio: Option<TwilioConfig>,
    /// Rate limits configuration
    pub rate_limits: RateLimits,
}

/// Application configuration settings
#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    /// Application metadata
    pub application: Application,
    /// Metrics configuration
    pub metrics: Metrics,
    /// Registration configuration
    pub registration: RegistrationConfig,
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    FileError(#[from] std::io::Error),
    #[error("Failed to parse config: {0}")]
    ParseError(String),
    #[error("Missing required config value: {0}")]
    MissingConfig(String),
}

impl From<config::ConfigError> for ConfigError {
    fn from(err: config::ConfigError) -> Self {
        ConfigError::ParseError(err.to_string())
    }
}

impl Config {
    /// Creates a new Config instance by loading and merging configuration from multiple sources.
    ///
    /// # Configuration Sources
    /// Configuration is loaded in the following order (later sources override earlier ones):
    /// 1. Base configuration (`application.yml`)
    /// 2. Environment variables
    ///
    /// # Errors
    /// Returns a `ConfigError` if:
    /// - Required configuration files cannot be read
    /// - Configuration values cannot be parsed
    /// - Required values are missing
    ///
    /// # Examples
    /// ```no_run
    /// use registration_service::config::Config;
    ///
    /// let config = Config::new().expect("Failed to load configuration");
    /// println!("Twilio Account SID: {}", config.registration().twilio.account_sid);
    /// ```
    pub fn new() -> Result<Self, ConfigError> {
        let builder = ConfigFile::builder()
            .add_source(File::with_name("config/application.yml"))
            .add_source(
                Environment::default()
                    .separator("_")
                    .try_parsing(true)
            )
            .set_override("registration.directory.entra_id.tenant_id", std::env::var("ENTRA_TENANT_ID").ok())?
            .set_override("registration.directory.entra_id.client_id", std::env::var("ENTRA_CLIENT_ID").ok())?
            .set_override("registration.directory.entra_id.client_secret", std::env::var("ENTRA_CLIENT_SECRET").ok())?;

        builder.build()?.try_deserialize().map_err(|e| ConfigError::ParseError(e.to_string()))
    }

    /// Returns the registration configuration.
    pub fn registration(&self) -> &RegistrationConfig {
        &self.registration
    }
}
