/// gRPC Service Module
///
/// Implements the gRPC service interface for the Signal Registration Service.
/// Handles registration requests, verification codes, and completion of registration
/// through a standardized protocol buffer interface.
///
/// # Features
/// - Async gRPC service implementation
/// - Protocol buffer message definitions
/// - Error handling and status codes
/// - Service health checks
///
/// # Copyright
/// Copyright (c) 2025 Signal Messenger, LLC
/// All rights reserved.
///
/// # License
/// Licensed under the AGPLv3 license.

use tonic::{Request, Response, Status};
use std::time::{SystemTime, Duration};
use std::sync::Arc;
use std::collections::HashMap;
use uuid::Uuid;

use crate::proto::registration::{
    registration_service_server::RegistrationService,
    StartRegistrationRequest, StartRegistrationResponse,
    VerifyCodeRequest, VerifyCodeResponse,
    CompleteRegistrationRequest, CompleteRegistrationResponse,
};

use crate::auth::ldap::{LdapClient, LdapError};
use crate::db::dynamodb::{DynamoDbClient, RegistrationRecord};
use crate::twilio::{TwilioClient, RateLimiter, VerificationChannel};
use tokio::sync::Mutex;

/// Session data for registration process
#[derive(Debug)]
struct Session {
    username: String,
    phone_number: String,
    verified: bool,
    created_at: SystemTime,
}

/// Maps LDAP errors to gRPC status codes
fn ldap_error_to_status(error: LdapError) -> Status {
    match error {
        LdapError::ConnectionFailed(msg) => Status::unavailable(msg),
        LdapError::BindFailed(msg) => Status::unauthenticated(msg),
        LdapError::SearchFailed(msg) => Status::internal(msg),
        LdapError::InvalidPhoneNumber(msg) => Status::invalid_argument(msg),
        LdapError::UserNotFound => Status::not_found("User not found"),
        LdapError::PhoneNumberMissing => Status::failed_precondition("Phone number not set"),
    }
}

/// Implementation of the Registration gRPC service
#[derive(Debug)]
pub struct RegistrationServer {
    ldap_client: Arc<LdapClient>,
    twilio_client: Arc<TwilioClient>,
    dynamodb_client: Arc<DynamoDbClient>,
    rate_limiter: Arc<RateLimiter>,
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    session_timeout: Duration,
}

#[tonic::async_trait]
impl RegistrationService for RegistrationServer {
    async fn start_registration(
        &self,
        request: Request<StartRegistrationRequest>,
    ) -> Result<Response<StartRegistrationResponse>, Status> {
        let req = request.into_inner();
        
        // Authenticate with LDAP and get phone number
        let phone_number = self.ldap_client
            .authenticate_and_get_phone(&req.username, &req.password)
            .await
            .map_err(ldap_error_to_status)?;
            
        // Check rate limit
        if !self.rate_limiter.check_rate_limit(&phone_number).await {
            return Err(Status::resource_exhausted("Too many verification attempts"));
        }
        
        // Start Twilio verification
        self.twilio_client
            .send_verification_code(&phone_number, match req.channel.as_str() {
                "sms" => VerificationChannel::Sms,
                "voice" => VerificationChannel::Voice,
                _ => return Err(Status::invalid_argument("Invalid channel. Must be 'sms' or 'voice'")),
            })
            .await
            .map_err(|e| Status::internal(format!("Failed to start verification: {}", e)))?;
            
        // Create session
        let session_id = Uuid::new_v4().to_string();
        let session = Session {
            username: req.username,
            phone_number: phone_number.clone(),
            verified: false,
            created_at: SystemTime::now(),
        };
        
        self.sessions.lock().await.insert(session_id.clone(), session);
        
        Ok(Response::new(StartRegistrationResponse {
            session_id,
            phone_number,
            verification_code_length: 6,
            verification_timeout_seconds: self.session_timeout.as_secs() as i32,
        }))
    }

    async fn verify_code(
        &self,
        request: Request<VerifyCodeRequest>,
    ) -> Result<Response<VerifyCodeResponse>, Status> {
        let req = request.into_inner();
        
        // Get session
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .get_mut(&req.session_id)
            .ok_or_else(|| Status::not_found("Session not found"))?;
            
        // Check if session is expired
        if SystemTime::now()
            .duration_since(session.created_at)
            .unwrap_or_default() > self.session_timeout
        {
            sessions.remove(&req.session_id);
            return Ok(Response::new(VerifyCodeResponse {
                success: false,
                message: "Session expired".to_string(),
                remaining_attempts: 0,
            }));
        }
        
        // Verify code with Twilio
        let valid = self.twilio_client
            .verify_code(&session.phone_number, &req.code)
            .await
            .map_err(|e| Status::internal(format!("Failed to verify code: {}", e)))?;
            
        if !valid {
            return Ok(Response::new(VerifyCodeResponse {
                success: false,
                message: "Invalid verification code".to_string(),
                remaining_attempts: 2, // TODO: Get actual remaining attempts from Twilio
            }));
        }
        
        // Mark session as verified
        session.verified = true;
        
        Ok(Response::new(VerifyCodeResponse {
            success: true,
            message: "Code verified successfully".to_string(),
            remaining_attempts: 0,
        }))
    }

    async fn complete_registration(
        &self,
        request: Request<CompleteRegistrationRequest>,
    ) -> Result<Response<CompleteRegistrationResponse>, Status> {
        let req = request.into_inner();
        
        // Get and remove session
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .remove(&req.session_id)
            .ok_or_else(|| Status::not_found("Session not found"))?;
            
        // Check if session is verified
        if !session.verified {
            return Ok(Response::new(CompleteRegistrationResponse {
                success: false,
                message: "Phone number not verified".to_string(),
            }));
        }
        
        // Save registration
        let record = RegistrationRecord {
            username: session.username,
            phone_number: session.phone_number,
            registration_id: req.registration_id.to_string(),
            created_at: SystemTime::now(),
        };
        
        match self.dynamodb_client.save_registration(record).await {
            Ok(_) => Ok(Response::new(CompleteRegistrationResponse {
                success: true,
                message: "Registration completed successfully".to_string(),
            })),
            Err(e) => Ok(Response::new(CompleteRegistrationResponse {
                success: false,
                message: format!("Failed to complete registration: {}", e),
            })),
        }
    }
}

impl RegistrationServer {
    pub fn new(
        ldap_client: LdapClient,
        twilio_client: TwilioClient,
        dynamodb_client: DynamoDbClient,
        rate_limiter: RateLimiter,
        session_timeout_secs: u64,
    ) -> Self {
        Self {
            ldap_client: Arc::new(ldap_client),
            twilio_client: Arc::new(twilio_client),
            dynamodb_client: Arc::new(dynamodb_client),
            rate_limiter: Arc::new(rate_limiter),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            session_timeout: Duration::from_secs(session_timeout_secs),
        }
    }
    
    #[allow(dead_code)]
    async fn cleanup_expired_sessions(&self) {
        let mut sessions = self.sessions.lock().await;
        sessions.retain(|_, session| {
            SystemTime::now()
                .duration_since(session.created_at)
                .unwrap_or_default() <= self.session_timeout
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::ldap::LdapConfig;
    use crate::db::dynamodb::DynamoDbConfig;
    use crate::twilio::{TwilioConfig, rate_limit::RateLimitConfig};
    use std::time::Duration;
    
    async fn setup_test_server() -> RegistrationServer {
        let ldap_config = LdapConfig {
            url: "ldap://localhost:389".to_string(),
            bind_dn: "cn=admin".to_string(),
            bind_password: "admin".to_string(),
            search_base: "dc=example,dc=com".to_string(),
            search_filter: "(&(objectClass=person)(uid=%s))".to_string(),
            phone_number_attribute: "mobile".to_string(),
            connection_pool_size: 5,
            timeout_secs: 30,
        };
        
        let twilio_config = TwilioConfig {
            account_sid: "test_account_sid".to_string(),
            auth_token: "test_auth_token".to_string(),
            verify_service_sid: "test_service_sid".to_string(),
            verification_timeout_secs: 300,
        };
        
        let dynamodb_config = DynamoDbConfig {
            table_name: "test_table".to_string(),
            region: "us-west-2".to_string(),
            endpoint: Some("http://localhost:8000".to_string()),
        };
        
        let rate_limit_config = RateLimitConfig {
            max_attempts: 3,
            window_secs: 300,
        };
        
        RegistrationServer::new(
            LdapClient::new(ldap_config).await.unwrap(),
            TwilioClient::new(twilio_config).unwrap(),
            DynamoDbClient::new(dynamodb_config.table_name, dynamodb_config.region).await.unwrap(),
            RateLimiter::new(rate_limit_config),
            300,
        )
    }
    
    #[tokio::test]
    async fn test_start_registration_success() {
        let server = setup_test_server().await;
        
        let request = Request::new(StartRegistrationRequest {
            username: "testuser".to_string(),
            password: "testpass".to_string(),
            channel: "sms".to_string(),
        });
        
        let response = server.start_registration(request).await.unwrap();
        let response = response.into_inner();
        
        assert!(!response.session_id.is_empty());
        assert!(!response.phone_number.is_empty());
        assert_eq!(response.verification_code_length, 6);
        assert_eq!(response.verification_timeout_seconds, 300);
    }
    
    #[tokio::test]
    async fn test_verify_code_success() {
        let server = setup_test_server().await;
        
        // First start a registration
        let start_request = Request::new(StartRegistrationRequest {
            username: "testuser".to_string(),
            password: "testpass".to_string(),
            channel: "sms".to_string(),
        });
        
        let start_response = server.start_registration(start_request).await.unwrap();
        let session_id = start_response.into_inner().session_id;
        
        // Then verify the code
        let verify_request = Request::new(VerifyCodeRequest {
            session_id,
            code: "123456".to_string(),
        });
        
        let verify_response = server.verify_code(verify_request).await.unwrap();
        let verify_response = verify_response.into_inner();
        
        assert!(verify_response.success);
        assert_eq!(verify_response.message, "Code verified successfully");
    }
    
    #[tokio::test]
    async fn test_complete_registration_success() {
        let server = setup_test_server().await;
        
        // Start registration and verify
        let start_request = Request::new(StartRegistrationRequest {
            username: "testuser".to_string(),
            password: "testpass".to_string(),
            channel: "sms".to_string(),
        });
        
        let start_response = server.start_registration(start_request).await.unwrap();
        let session_id = start_response.into_inner().session_id;
        
        let verify_request = Request::new(VerifyCodeRequest {
            session_id: session_id.clone(),
            code: "123456".to_string(),
        });
        
        server.verify_code(verify_request).await.unwrap();
        
        // Complete registration
        let complete_request = Request::new(CompleteRegistrationRequest {
            session_id,
            registration_id: 12345,
            device_id: 1,
            identity_key: "test_identity_key".to_string(),
        });
        
        let complete_response = server.complete_registration(complete_request).await.unwrap();
        let complete_response = complete_response.into_inner();
        
        assert!(complete_response.success);
        assert_eq!(complete_response.message, "Registration completed successfully");
    }
}
