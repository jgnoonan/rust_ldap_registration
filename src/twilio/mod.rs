/// Twilio Integration Module
///
/// Provides Twilio SMS and Voice verification services for the Signal Registration Service.
/// This module handles sending verification codes and verifying user responses through
/// Twilio's Verify API.
///
/// # Features
/// - SMS verification
/// - Voice verification
/// - Rate limiting
/// - Configurable timeouts and retries
/// - Test mode support
///
/// # Copyright
/// Copyright (c) 2025 Signal Messenger, LLC
/// All rights reserved.
///
/// # License
/// Licensed under the AGPLv3 license.

use reqwest::Client as HttpClient;
use anyhow::Result;
use std::time::Duration;
use tracing::{error, info};
use serde::Deserialize;

pub mod rate_limit;
pub use rate_limit::RateLimiter;

/// Configuration for Twilio API connection
#[derive(Debug, Clone)]
pub struct TwilioConfig {
    /// Twilio Account SID
    pub account_sid: String,
    /// Twilio Auth Token
    pub auth_token: String,
    /// Twilio Verify Service SID
    pub verify_service_sid: String,
    /// Verification timeout in seconds
    pub verification_timeout_secs: u64,
}

/// Verification channel type
#[derive(Debug, Clone, Copy)]
pub enum VerificationChannel {
    /// SMS verification
    Sms,
    /// Voice verification
    Voice,
}

impl ToString for VerificationChannel {
    fn to_string(&self) -> String {
        match self {
            Self::Sms => "sms".to_string(),
            Self::Voice => "voice".to_string(),
        }
    }
}

/// Client for Twilio Verify API operations
#[derive(Debug)]
pub struct TwilioClient {
    account_sid: String,
    auth_token: String,
    verification_service_sid: String,
    http_client: HttpClient,
}

impl TwilioClient {
    /// Creates a new Twilio client with the specified configuration
    ///
    /// # Arguments
    /// * `config` - Twilio configuration including credentials and service SID
    ///
    /// # Returns
    /// * `Result<TwilioClient>` - New Twilio client instance or error if configuration fails
    ///
    /// # Examples
    /// ```no_run
    /// use registration_service::twilio::{TwilioClient, TwilioConfig};
    ///
    /// let config = TwilioConfig {
    ///     account_sid: "AC123...".to_string(),
    ///     auth_token: "auth123...".to_string(),
    ///     verify_service_sid: "VA123...".to_string(),
    ///     verification_timeout_secs: 300,
    /// };
    ///
    /// let client = TwilioClient::new(config).expect("Failed to create Twilio client");
    /// ```
    pub fn new(config: TwilioConfig) -> Result<Self> {
        let http_client = HttpClient::builder()
            .timeout(Duration::from_secs(config.verification_timeout_secs))
            .build()?;
            
        Ok(Self {
            account_sid: config.account_sid,
            auth_token: config.auth_token,
            verification_service_sid: config.verify_service_sid,
            http_client,
        })
    }
    
    /// Sends a verification code via SMS or voice
    ///
    /// # Arguments
    /// * `phone_number` - Target phone number in E.164 format
    /// * `channel` - Verification channel (SMS or Voice)
    ///
    /// # Returns
    /// * `Result<()>` - Success or error if sending fails
    ///
    /// # Examples
    /// ```no_run
    /// # use registration_service::twilio::{TwilioClient, VerificationChannel};
    /// # let client = get_twilio_client();
    /// client.send_verification_code("+1234567890", VerificationChannel::Sms).await?;
    /// # async fn get_twilio_client() -> TwilioClient { unimplemented!() }
    /// ```
    pub async fn send_verification_code(&self, phone_number: &str, channel: VerificationChannel) -> Result<()> {
        let url = format!(
            "https://verify.twilio.com/v2/Services/{}/Verifications",
            self.verification_service_sid
        );
        
        let params = [
            ("To", phone_number),
            ("Channel", &channel.to_string()),
        ];
        
        let response = self.http_client
            .post(&url)
            .basic_auth(&self.account_sid, Some(&self.auth_token))
            .form(&params)
            .send()
            .await?;
            
        if !response.status().is_success() {
            let error_text = response.text().await?;
            error!("Twilio verification request failed: {}", error_text);
            anyhow::bail!("Failed to send verification code: {}", error_text);
        }
        
        info!("Sent verification code to {}", phone_number);
        Ok(())
    }
    
    /// Verifies a code submitted by the user
    ///
    /// # Arguments
    /// * `phone_number` - Phone number in E.164 format
    /// * `code` - Verification code submitted by user
    ///
    /// # Returns
    /// * `Result<bool>` - True if verification succeeds, false otherwise
    ///
    /// # Examples
    /// ```no_run
    /// # use registration_service::twilio::TwilioClient;
    /// # let client = get_twilio_client();
    /// if client.verify_code("+1234567890", "123456").await? {
    ///     println!("Verification successful");
    /// }
    /// # async fn get_twilio_client() -> TwilioClient { unimplemented!() }
    /// ```
    pub async fn verify_code(&self, phone_number: &str, code: &str) -> Result<bool> {
        let url = format!(
            "https://verify.twilio.com/v2/Services/{}/VerificationCheck",
            self.verification_service_sid
        );
        
        let params = [
            ("To", phone_number),
            ("Code", code),
        ];
        
        let response = self.http_client
            .post(&url)
            .basic_auth(&self.account_sid, Some(&self.auth_token))
            .form(&params)
            .send()
            .await?;
            
        if !response.status().is_success() {
            let error_text = response.text().await?;
            error!("Twilio verification check failed: {}", error_text);
            anyhow::bail!("Failed to verify code: {}", error_text);
        }
        
        #[derive(Deserialize)]
        struct VerificationCheck {
            status: String,
        }
        
        let check: VerificationCheck = response.json().await?;
        Ok(check.status == "approved")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;
    
    #[tokio::test]
    async fn test_start_verification() {
        let mut mock_server = Server::new();
        
        let config = TwilioConfig {
            account_sid: "test_account_sid".to_string(),
            auth_token: "test_auth_token".to_string(),
            verify_service_sid: "test_service_sid".to_string(),
            verification_timeout_secs: 300,
        };

        // Create a mock for the verification request
        let mock = mock_server
            .mock("POST", "/v2/Services/test_service_sid/Verifications")
            .match_body(r#"{"To":"+1234567890","Channel":"sms"}"#)
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(r#"{"status": "pending"}"#)
            .create();

        let client = TwilioClient::new(config).expect("Failed to create client");
        let result = client.send_verification_code("+1234567890", VerificationChannel::Sms).await;
        
        assert!(result.is_ok());
        mock.assert();
    }
    
    #[tokio::test]
    async fn test_check_verification() {
        // Add test implementation
    }
}