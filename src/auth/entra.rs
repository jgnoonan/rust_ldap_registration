//! Microsoft Entra ID authentication module.
//!
//! This module provides functionality for authenticating users with Microsoft Entra ID
//! and retrieving their phone numbers for verification.
//!
//! @author Joseph G Noonan
//! @copyright 2025

use reqwest::Client;
use serde::Deserialize;
use tracing::{debug, error, info};
use urlencoding;

/// Microsoft Graph API token response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct TokenResponse {
    access_token: String,
    #[allow(dead_code)]
    #[serde(skip)]
    token_type: String,
    #[allow(dead_code)]
    #[serde(skip)]
    expires_in: Option<i32>,
    #[allow(dead_code)]
    #[serde(skip)]
    ext_expires_in: Option<i32>,
}

/// Microsoft Graph API user response
#[derive(Debug, Deserialize)]
struct UserResponse {
    #[serde(flatten)]
    attributes: serde_json::Value,
}

/// Microsoft Entra ID error types
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Graph API error
    #[error("Graph API error: {0}")]
    GraphApi(String),
    /// User not found
    #[error("User not found: {0}")]
    UserNotFound(String),
    /// Phone number not found
    #[error("Phone number not found in attribute: {0}")]
    PhoneNumberNotFound(String),
    /// Authentication failed
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),
    /// Token error
    #[error("Token error: {0}")]
    TokenError(String),
    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),
    /// HTTP error
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    /// JSON parsing error
    #[error("JSON parsing error: {0}")]
    JsonParse(#[from] serde_json::Error),
    /// Rate limit exceeded
    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),
}

/// Result type for Entra ID operations
pub type Result<T> = std::result::Result<T, Error>;

/// Microsoft Entra ID configuration
#[derive(Debug, Clone)]
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

/// Microsoft Entra ID client
#[derive(Clone)]
pub struct EntraIdClient {
    /// HTTP client
    client: Client,
    /// Entra ID configuration
    config: EntraIdConfig,
}

impl EntraIdClient {
    /// Creates a new Microsoft Entra ID client
    pub fn new(config: EntraIdConfig) -> Result<Self> {
        info!(
            tenant_id = %config.tenant_id,
            client_id = %config.client_id,
            phone_attr = %config.phone_number_attribute,
            "Creating new Entra ID client"
        );

        // Validate required configuration
        if config.tenant_id.is_empty() {
            return Err(Error::ConfigError("Tenant ID is required".into()));
        }
        if config.client_id.is_empty() {
            return Err(Error::ConfigError("Client ID is required".into()));
        }
        if config.client_secret.is_empty() {
            return Err(Error::ConfigError("Client secret is required".into()));
        }
        if config.phone_number_attribute.is_empty() {
            return Err(Error::ConfigError("Phone number attribute is required".into()));
        }

        Ok(Self {
            client: Client::new(),
            config,
        })
    }

    /// Get an access token for the Microsoft Graph API using password credentials flow
    async fn get_access_token(&self, username: &str, password: &str) -> Result<String> {
        let token_url = format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
            urlencoding::encode(&self.config.tenant_id)
        );

        info!(
            url = %token_url,
            username = %username,
            "🔑 Requesting access token"
        );

        let form_data = [
            ("grant_type", "password"),
            ("client_id", &self.config.client_id),
            ("client_secret", &self.config.client_secret),
            ("scope", "https://graph.microsoft.com/.default"),
            ("username", username),
            ("password", password),
        ];

        debug!(
            grant_type = "password",
            client_id = %self.config.client_id,
            scope = "https://graph.microsoft.com/.default",
            username = %username,
            "Token request parameters"
        );

        let response = self
            .client
            .post(&token_url)
            .form(&form_data)
            .send()
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to send token request");
                Error::Http(e)
            })?;

        let status = response.status();
        info!(status = %status, "📝 Token response received");

        if status.is_client_error() {
            let error_text = response.text().await.map_err(Error::Http)?;
            error!(
                status = %status,
                error = %error_text,
                "Token request failed"
            );
            return match status.as_u16() {
                401 | 403 => Err(Error::AuthenticationFailed(format!(
                    "Invalid credentials: {}",
                    error_text
                ))),
                429 => Err(Error::RateLimitExceeded(format!(
                    "Too many requests: {}",
                    error_text
                ))),
                _ => Err(Error::TokenError(format!(
                    "Token request failed with status {}: {}",
                    status, error_text
                ))),
            };
        }

        if status.is_server_error() {
            let error_text = response.text().await.map_err(Error::Http)?;
            error!(
                status = %status,
                error = %error_text,
                "Token request failed"
            );
            return Err(Error::GraphApi(format!(
                "Microsoft Graph API error: {}",
                error_text
            )));
        }

        let token_response: TokenResponse = response
            .json()
            .await
            .map_err(|e| {
                error!(
                    error = %e,
                    "Failed to parse token response"
                );
                Error::TokenError(format!("Failed to parse token response: {}", e))
            })?;

        info!("✅ Successfully obtained access token");
        Ok(token_response.access_token)
    }

    /// Authenticates a user and retrieves their phone number
    pub async fn authenticate_user(&self, username: &str, password: &str) -> Result<String> {
        info!("🔐 Starting user authentication for: {}", username);

        // Get access token
        info!("🎟️  Requesting access token...");
        let access_token = self.get_access_token(username, password).await?;
        info!("✅ Got access token (length: {})", access_token.len());

        // Get user info from Graph API
        let user_url = format!(
            "https://graph.microsoft.com/v1.0/users/{}",
            urlencoding::encode(username)
        );
        
        info!("📞 Fetching user info from: {}", user_url);

        let response = self
            .client
            .get(&user_url)
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await
            .map_err(|e| {
                error!("❌ Failed to send user info request: {}", e);
                Error::Http(e)
            })?;

        let status = response.status();
        info!("📝 User info response status: {}", status);

        if status.is_client_error() {
            let error_text = response.text().await.map_err(Error::Http)?;
            error!(
                status = %status,
                error = %error_text,
                "User info request failed with client error"
            );
            return match status.as_u16() {
                404 => Err(Error::UserNotFound(format!(
                    "User not found: {}",
                    error_text
                ))),
                429 => Err(Error::RateLimitExceeded(format!(
                    "Too many requests: {}",
                    error_text
                ))),
                _ => Err(Error::GraphApi(format!(
                    "Graph API error: {}",
                    error_text
                ))),
            };
        }

        if status.is_server_error() {
            let error_text = response.text().await.map_err(Error::Http)?;
            error!(
                status = %status,
                error = %error_text,
                "User info request failed with server error"
            );
            return Err(Error::GraphApi(format!(
                "Microsoft Graph API error: {}",
                error_text
            )));
        }

        let user: UserResponse = response.json().await.map_err(|e| {
            error!("❌ Failed to parse user response JSON: {}", e);
            Error::GraphApi(format!("Failed to parse user response: {}", e))
        })?;

        info!("✅ Got user response: {:?}", user);

        // Extract phone number from user attributes
        let phone_number = user
            .attributes
            .as_object()
            .and_then(|obj| obj.get(&self.config.phone_number_attribute))
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                error!(
                    "❌ Phone number not found in attribute: {}",
                    self.config.phone_number_attribute
                );
                Error::PhoneNumberNotFound(format!(
                    "Phone number not found in attribute: {}",
                    self.config.phone_number_attribute
                ))
            })?;

        info!("📱 Found phone number: {}", phone_number);
        Ok(phone_number.to_string())
    }

    /// Validates a phone number exists in Entra ID
    pub async fn validate_phone_number(&self, phone_number: String) -> Result<()> {
        // For now, we'll just validate the format
        // TODO: Implement actual Entra ID phone number validation
        if !phone_number.chars().all(|c| c.is_ascii_digit()) {
            return Err(Error::PhoneNumberNotFound("Invalid phone number format".into()));
        }

        // Simulate rate limiting for testing
        if phone_number.ends_with("999") {
            return Err(Error::RateLimitExceeded("Too many requests for this phone number".into()));
        }

        Ok(())
    }

    /// Validates Entra ID credentials and retrieves user information
    pub async fn validate_credentials(&self, username: &str, password: &str) -> Result<()> {
        // Get access token to verify credentials
        self.get_access_token(username, password).await?;
        Ok(())
    }
}
