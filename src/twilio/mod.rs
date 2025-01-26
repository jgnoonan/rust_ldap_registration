use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{info, error};

#[derive(Debug, Clone)]
pub struct TwilioConfig {
    pub account_sid: String,
    pub auth_token: String,
    pub verify_service_sid: String,
    pub verification_timeout_secs: u64,
}

#[derive(Debug, Serialize)]
struct VerificationRequest<'a> {
    #[serde(rename = "To")]
    to: &'a str,
    #[serde(rename = "Channel")]
    channel: &'a str,
}

#[derive(Debug, Deserialize)]
struct VerificationResponse {
    sid: String,
    status: String,
}

#[derive(Debug, Serialize)]
struct VerificationCheckRequest<'a> {
    #[serde(rename = "To")]
    to: &'a str,
    #[serde(rename = "Code")]
    code: &'a str,
}

#[derive(Debug, Deserialize)]
struct VerificationCheckResponse {
    status: String,
    valid: bool,
}

pub struct TwilioClient {
    config: TwilioConfig,
    client: Client,
    base_url: String,
}

impl TwilioClient {
    pub fn new(config: TwilioConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;
            
        let base_url = format!(
            "https://verify.twilio.com/v2/Services/{}/",
            config.verify_service_sid
        );
        
        Ok(Self {
            config,
            client,
            base_url,
        })
    }
    
    pub async fn start_verification(&self, phone_number: &str, channel: &str) -> Result<bool> {
        let url = format!("{}Verifications", self.base_url);
        
        let request = VerificationRequest {
            to: phone_number,
            channel,
        };
        
        let response = self.client
            .post(&url)
            .basic_auth(&self.config.account_sid, Some(&self.config.auth_token))
            .form(&request)
            .send()
            .await?;
            
        if !response.status().is_success() {
            error!(
                "Failed to start verification for {}: {}",
                phone_number,
                response.text().await?
            );
            return Ok(false);
        }
        
        let verification: VerificationResponse = response.json().await?;
        info!(
            "Started {} verification for {}: {}",
            channel, phone_number, verification.sid
        );
        
        Ok(verification.status == "pending")
    }
    
    pub async fn check_verification(&self, phone_number: &str, code: &str) -> Result<bool> {
        let url = format!("{}VerificationCheck", self.base_url);
        
        let request = VerificationCheckRequest {
            to: phone_number,
            code,
        };
        
        let response = self.client
            .post(&url)
            .basic_auth(&self.config.account_sid, Some(&self.config.auth_token))
            .form(&request)
            .send()
            .await?;
            
        if !response.status().is_success() {
            error!(
                "Failed to check verification for {}: {}",
                phone_number,
                response.text().await?
            );
            return Ok(false);
        }
        
        let check: VerificationCheckResponse = response.json().await?;
        info!(
            "Checked verification for {}: status={}, valid={}",
            phone_number, check.status, check.valid
        );
        
        Ok(check.valid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::mock;
    
    #[tokio::test]
    async fn test_start_verification() {
        let mock_server = mockito::Server::new();
        
        let config = TwilioConfig {
            account_sid: "test_account_sid".to_string(),
            auth_token: "test_auth_token".to_string(),
            verify_service_sid: "test_service_sid".to_string(),
            verification_timeout_secs: 300,
        };
        
        // Add mock implementation
    }
    
    #[tokio::test]
    async fn test_check_verification() {
        // Add test implementation
    }
}
