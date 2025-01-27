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

use serde::Deserialize;
use std::env;
use std::path::PathBuf;
use config::{Config as ConfigFile, ConfigError, Environment, File};

/// Application metadata configuration
#[derive(Debug, Deserialize)]
pub struct Application {
    /// Name of the application
    pub name: String,
}

/// Metrics configuration
#[derive(Debug, Deserialize)]
pub struct Metrics {
    /// Whether metrics collection is enabled
    pub enabled: bool,
    /// Metrics export configuration
    pub export: MetricsExport,
}

/// Metrics export configuration
#[derive(Debug, Deserialize)]
pub struct MetricsExport {
    /// Datadog-specific configuration
    pub datadog: DatadogConfig,
}

/// Datadog configuration
#[derive(Debug, Deserialize)]
pub struct DatadogConfig {
    /// Whether Datadog export is enabled
    pub enabled: bool,
}

/// LDAP connection and authentication configuration
#[derive(Debug, Deserialize)]
pub struct LdapConfig {
    /// LDAP server URL
    pub url: String,
    /// Base DN for LDAP queries
    pub base_dn: String,
    /// Whether to use SSL for LDAP connection
    pub use_ssl: bool,
    /// Path to trust store file
    pub trust_store: Option<String>,
    /// Password for trust store
    pub trust_store_password: Option<String>,
    /// Trust store type (e.g., "JKS")
    pub trust_store_type: Option<String>,
    /// Whether to verify hostname in SSL cert
    pub hostname_verification: bool,
    /// Connection timeout in milliseconds
    pub connection_timeout: u64,
    /// Read timeout in milliseconds
    pub read_timeout: u64,
    /// Minimum number of connections in pool
    pub min_pool_size: u32,
    /// Maximum number of connections in pool
    pub max_pool_size: u32,
    /// Pool timeout in milliseconds
    pub pool_timeout: u64,
    /// Maximum number of connection retries
    pub max_retries: u32,
    /// LDAP filter for user lookup
    pub user_filter: String,
    /// DN for binding to LDAP
    pub bind_dn: String,
    /// Password for binding to LDAP
    pub bind_password: String,
    /// LDAP attribute containing phone number
    pub phone_number_attribute: String,
    /// LDAP attribute for username
    pub username_attribute: String,
}

/// Rate limiting configuration
#[derive(Debug, Deserialize, Clone)]
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
#[derive(Debug, Deserialize, Clone)]
pub struct DelayConfig {
    /// Number of delays
    pub delays: u64,
}

/// Voice delay configuration
#[derive(Debug, Deserialize, Clone)]
pub struct VoiceDelayConfig {
    /// Delay after first SMS
    pub delay_after_first_sms: u64,
    /// Number of delays
    pub delays: u64,
    /// Maximum number of attempts allowed
    pub max_attempts: u32,
}

/// Leaky bucket configuration
#[derive(Debug, Deserialize, Clone)]
pub struct LeakyBucketConfig {
    /// Session creation configuration
    pub session_creation: SessionCreationConfig,
}

/// Session creation configuration
#[derive(Debug, Deserialize, Clone)]
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

/// DynamoDB configuration
#[derive(Debug, Deserialize, Clone)]
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
#[derive(Debug, Deserialize)]
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
#[derive(Debug, Deserialize)]
pub struct Selection {
    /// SMS transport configuration
    pub sms: TransportConfig,
    /// Voice transport configuration
    pub voice: VoiceTransportConfig,
}

/// Transport configuration
#[derive(Debug, Deserialize)]
pub struct TransportConfig {
    /// Transport type
    pub transport: String,
    /// Fallback senders for transport
    pub fallback_senders: Vec<String>,
}

/// Voice transport configuration
#[derive(Debug, Deserialize)]
pub struct VoiceTransportConfig {
    /// Transport type
    pub transport: String,
    /// Fallback senders for transport
    pub fallback_senders: Vec<String>,
    /// Default weights for transport
    pub default_weights: std::collections::HashMap<String, u32>,
}

/// gRPC server configuration
#[derive(Debug, Deserialize)]
pub struct GrpcConfig {
    /// Server configuration
    pub server: ServerConfig,
    /// Timeout in seconds for gRPC operations
    pub timeout_secs: u64,
}

/// Server configuration
#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    /// Endpoint for server
    pub endpoint: String,
    /// Port for server
    pub port: u16,
    /// Session timeout in seconds
    pub timeout_secs: u64,
}

/// Registration configuration
#[derive(Debug, Deserialize)]
pub struct RegistrationConfig {
    pub use_ldap: bool,
    pub ldap: LdapConfig,
    pub grpc: GrpcConfig,
    pub twilio: TwilioConfig,
    pub dynamodb: DynamoDbConfig,
    pub rate_limits: RateLimits,
}

/// Environment configuration
#[derive(Debug, Deserialize)]
pub struct EnvironmentConfig {
    pub config: ConfigWrapper,
}

/// Config wrapper
#[derive(Debug, Deserialize)]
pub struct ConfigWrapper {
    pub registration: RegistrationConfig,
}

/// Environments configuration
#[derive(Debug, Deserialize)]
pub struct Environments {
    pub development: EnvironmentConfig,
    pub production: EnvironmentConfig,
}

/// Application configuration settings
#[derive(Debug, Deserialize)]
pub struct Config {
    pub application: Application,
    pub metrics: Metrics,
    pub environments: Environments,
}

impl Config {
    /// Creates a new Config instance by loading and merging configuration from multiple sources.
    ///
    /// # Configuration Sources
    /// Configuration is loaded in the following order (later sources override earlier ones):
    /// 1. Base configuration (`application.yml`)
    /// 2. Environment-specific configuration (`application-{environment}.yml`)
    /// 3. Local configuration (`application-local.yml`)
    /// 4. Environment variables (prefixed with `APP_`)
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
        let environment = env::var("APP_ENV").unwrap_or_else(|_| "development".to_string());
        
        let config_dir = PathBuf::from("config");
        let config = ConfigFile::builder()
            .add_source(File::from(config_dir.join("application.yml")))
            .add_source(File::from(config_dir.join(format!("application-{}.yml", environment))).required(false))
            .add_source(File::from(config_dir.join("application-local.yml")).required(false))
            .add_source(Environment::with_prefix("APP").separator("_"))
            .build()?;
            
        // Deserialize the configuration
        config.try_deserialize()
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
        if env::var("APP_ENV").unwrap_or_else(|_| "development".to_string()) == "development" {
            &self.environments.development.config.registration
        } else {
            &self.environments.production.config.registration
        }
    }
}
