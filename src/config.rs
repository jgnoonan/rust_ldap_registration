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
    /// Check verification code rate limiting
    pub check_verification_code: DelayConfig,
    /// Leaky bucket rate limiting
    pub leaky_bucket: LeakyBucketConfig,
    /// Send SMS verification code rate limiting
    pub send_sms_verification_code: DelayConfig,
    /// Send voice verification code rate limiting
    pub send_voice_verification_code: VoiceDelayConfig,
}

/// Delay configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DelayConfig {
    /// Number of delays
    pub delays: u64,
}

/// Voice delay configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct VoiceDelayConfig {
    /// Delay after first SMS
    pub delay_after_first_sms: u64,
    /// Number of delays
    pub delays: u64,
    /// Maximum number of attempts allowed
    pub max_attempts: u32,
}

/// Leaky bucket configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LeakyBucketConfig {
    /// Session creation configuration
    pub session_creation: SessionCreationConfig,
}

/// Session creation configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SessionCreationConfig {
    /// Name of the session creation configuration
    pub name: String,
    /// Maximum capacity of the session creation configuration
    pub max_capacity: u32,
    /// Permit regeneration period of the session creation configuration
    pub permit_regeneration_period: u64,
    /// Minimum delay of the session creation configuration
    pub min_delay: u64,
}

/// Rate limit configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateLimitConfig {
    /// Maximum number of requests per window
    pub max_requests: u32,
    /// Time window in seconds
    #[serde(rename = "window")]
    window_secs: u64,
}

impl RateLimitConfig {
    pub fn window(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.window_secs)
    }
}

/// LDAP connection and authentication configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct LdapConfig {
    /// LDAP server URL
    pub url: String,
    /// Bind DN for authentication
    pub bind_dn: String,
    /// Password for bind DN
    #[serde(skip_serializing)]
    pub bind_password: String,
    /// Base DN for searches
    pub base_dn: String,
    /// Attribute containing username
    pub username_attribute: String,
    /// Attribute containing phone number
    pub phone_number_attribute: String,
}

impl LdapConfig {
    /// Validates the configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.url.is_empty() {
            return Err(ConfigError::MissingConfig("LDAP URL is required".to_string()));
        }
        if self.bind_dn.is_empty() {
            return Err(ConfigError::MissingConfig("Bind DN is required".to_string()));
        }
        if self.bind_password.is_empty() {
            return Err(ConfigError::MissingConfig("Bind password is required".to_string()));
        }
        if self.base_dn.is_empty() {
            return Err(ConfigError::MissingConfig("Base DN is required".to_string()));
        }
        Ok(())
    }
}

/// DynamoDB configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DynamoDbConfig {
    /// Whether DynamoDB is enabled
    pub enabled: bool,
    /// DynamoDB table name
    pub table_name: String,
    /// AWS region
    pub region: String,
    /// DynamoDB endpoint (optional, for local development)
    pub endpoint: Option<String>,
}

/// Twilio configuration
#[derive(Debug, Deserialize, Serialize)]
pub struct TwilioConfig {
    /// Whether Twilio is enabled
    pub enabled: bool,
    /// Account SID for Twilio
    pub account_sid: Option<String>,
    /// Auth token for Twilio
    pub auth_token: Option<String>,
    /// Verify service SID for Twilio
    pub verify_service_sid: Option<String>,
    /// Verification timeout in seconds for Twilio
    pub verification_timeout_secs: u64,
}

/// Transport selection configuration
#[derive(Debug, Deserialize, Serialize)]
pub struct Selection {
    /// SMS transport configuration
    pub sms: TransportConfig,
    /// Voice transport configuration
    pub voice: VoiceTransportConfig,
}

/// Transport configuration
#[derive(Debug, Deserialize, Serialize)]
pub struct TransportConfig {
    /// Transport type
    pub transport: String,
    /// Fallback senders for transport
    pub fallback_senders: Vec<String>,
}

/// Voice transport configuration
#[derive(Debug, Deserialize, Serialize)]
pub struct VoiceTransportConfig {
    /// Transport type
    pub transport: String,
    /// Fallback senders for transport
    pub fallback_senders: Vec<String>,
    /// Default weights for transport
    pub default_weights: std::collections::HashMap<String, u32>,
}

/// gRPC server configuration
#[derive(Debug, Deserialize, Serialize)]
pub struct GrpcConfig {
    /// Server configuration
    pub server: ServerConfig,
    /// Timeout in seconds for gRPC operations
    pub timeout_secs: u64,
}

/// Server configuration
#[derive(Debug, Deserialize, Serialize)]
pub struct ServerConfig {
    /// Endpoint for server
    pub endpoint: String,
    /// Port for server
    pub port: u16,
    /// Session timeout in seconds
    pub timeout_secs: u64,
}

/// Registration configuration
#[derive(Debug, Deserialize, Serialize)]
pub struct RegistrationConfig {
    pub use_ldap: bool,
    pub ldap: LdapConfig,
    pub grpc: GrpcConfig,
    pub twilio: TwilioConfig,
    pub dynamodb: DynamoDbConfig,
    pub rate_limits: RateLimits,
}

/// Environment configuration
#[derive(Debug, Deserialize, Serialize)]
pub struct EnvironmentConfig {
    pub config: ConfigWrapper,
}

/// Config wrapper
#[derive(Debug, Deserialize, Serialize)]
pub struct ConfigWrapper {
    pub registration: RegistrationConfig,
}

/// Environments configuration
#[derive(Debug, Deserialize, Serialize)]
pub struct Environments {
    pub development: EnvironmentConfig,
    pub production: EnvironmentConfig,
}

/// Application configuration settings
#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub application: Application,
    pub metrics: Metrics,
    pub environments: Environments,
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
    /// 2. Environment variables (prefixed with `APP_`)
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
    /// println!("LDAP URL: {}", config.registration().ldap.url);
    /// ```
    pub fn new() -> Result<Self, ConfigError> {
        let builder = ConfigFile::builder()
            .add_source(File::with_name("config/application.yml"))
            .add_source(Environment::with_prefix("APP"));

        let config = builder.build()?;
        config.try_deserialize().map_err(|e| ConfigError::ParseError(e.to_string()))
    }

    /// Returns the registration configuration.
    ///
    /// # Returns
    /// A reference to the registration configuration
    ///
    /// # Examples
    /// ```
    /// use registration_service::config::Config;
    ///
    /// let config = Config::new().unwrap();
    /// let registration_config = config.registration();
    /// assert!(registration_config.use_ldap);
    /// ```
    pub fn registration(&self) -> &RegistrationConfig {
        if std::env::var("APP_ENV").unwrap_or_else(|_| "development".to_string()) == "development" {
            &self.environments.development.config.registration
        } else {
            &self.environments.production.config.registration
        }
    }
}
