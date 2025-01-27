//! Twilio client for phone number verification.
//!
//! This module provides integration with Twilio's Verify API for phone number
//! verification. It handles sending verification codes and validating responses
//! from users. The module supports both production and test modes for development.
//!
//! @author Joseph G Noonan
//! @copyright 2025

use reqwest::Client as HttpClient;
use anyhow::Result;
use std::time::Duration;
use tracing::{error, info};
use serde::Deserialize;

pub mod rate_limit;
pub use rate_limit::RateLimiter;

/// Configuration for Twilio API connection.
#[derive(Debug, Clone)]
pub struct TwilioConfig {
    /// Twilio account SID
    pub account_sid: String,
    /// Twilio auth token
    pub auth_token: String,
    /// Verify service SID
    pub verify_service_sid: String,
    /// Timeout for verification codes in seconds
    pub verification_timeout_secs: u64,
    /// Whether to operate in test mode
    pub test_mode: bool,
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

/// Client for Twilio Verify API operations.
///
/// Provides methods for sending verification codes and checking responses
/// via Twilio's Verify API. Supports both production and test modes for
/// development environments.
#[derive(Debug)]
pub struct TwilioClient {
    account_sid: String,
    auth_token: String,
    verification_service_sid: String,
    http_client: HttpClient,
    test_mode: bool,
    test_ldap_phone: Option<String>,
}

impl TwilioClient {
    /// Creates a new Twilio client instance.
    ///
    /// # Arguments
    /// * `config` - Twilio configuration including credentials
    ///
    /// # Returns
    /// * `Result<Self>` - New client instance or error if initialization fails
    pub fn new(config: TwilioConfig) -> Result<Self> {
        let http_client = HttpClient::builder()
            .timeout(Duration::from_secs(config.verification_timeout_secs))
            .build()?;
            
        Ok(Self {
            account_sid: config.account_sid,
            auth_token: config.auth_token,
            verification_service_sid: config.verify_service_sid,
            http_client,
            test_mode: config.test_mode,
            test_ldap_phone: None,
        })
    }

    /// Sends a verification code to a phone number.
    ///
    /// # Arguments
    /// * `phone_number` - Target phone number for verification
    /// * `channel` - Verification channel (SMS or Voice)
    ///
    /// # Returns
    /// * `Result<()>` - Success or error if sending fails
    pub async fn send_verification_code(&self, phone_number: &str, channel: VerificationChannel) -> Result<()> {
        if self.test_mode {
            if let Some(ldap_phone) = &self.test_ldap_phone {
                info!("Test mode: Verification code will be last 6 digits of LDAP phone number: {}", ldap_phone);
                return Ok(());
            } else {
                anyhow::bail!("Test mode requires LDAP phone number to be set");
            }
        }

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

    /// Verifies a code submitted by a user.
    ///
    /// # Arguments
    /// * `phone_number` - Phone number being verified
    /// * `code` - Verification code submitted by user
    ///
    /// # Returns
    /// * `Result<bool>` - True if code is valid
    pub async fn verify_code(&self, phone_number: &str, code: &str) -> Result<bool> {
        if self.test_mode {
            let ldap_phone = self.test_ldap_phone.as_ref()
                .ok_or_else(|| anyhow::anyhow!("Test mode requires LDAP phone number to be set"))?;
                
            // Extract last 6 digits from LDAP phone number
            let expected_code = ldap_phone
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect::<String>()
                .chars()
                .rev()
                .take(6)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>();
            
            info!("Test mode: Comparing code {} with expected {} (from LDAP phone: {})", 
                  code, expected_code, ldap_phone);
            return Ok(code == expected_code);
        }

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

    /// Stores a phone number for test mode verification.
    ///
    /// # Arguments
    /// * `phone` - Phone number to store for testing
    pub fn set_test_ldap_phone(&mut self, phone: String) {
        self.test_ldap_phone = Some(phone);
    }
}