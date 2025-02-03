//! gRPC server implementation for the Signal Registration Service.
//!
//! This module implements the gRPC service endpoints defined in the proto files,
//! handling user registration and Entra ID validation requests. It manages user sessions,
//! rate limiting, and coordinates between various backend services (Entra ID).
//!
//! @author Joseph G Noonan
//! @copyright 2025

use std::sync::Arc;
use std::time::SystemTime;
use tracing::{info, warn, error};

use tonic::{Request, Response, Status};
use tonic::metadata::MetadataMap;
use rand::prelude::*;

use crate::auth::entra::EntraIdClient;
use crate::session::SessionStore;
use crate::proto::{
    registration_service_server::RegistrationService,
    CreateRegistrationSessionRequest,
    CreateRegistrationSessionResponse,
    GetRegistrationSessionMetadataRequest,
    GetRegistrationSessionMetadataResponse,
    SendVerificationCodeRequest,
    SendVerificationCodeResponse,
    CheckVerificationCodeRequest,
    CheckVerificationCodeResponse,
    CreateRegistrationSessionError,
    CreateRegistrationSessionErrorType,
    SendVerificationCodeError,
    SendVerificationCodeErrorType,
    CheckVerificationCodeError,
    CheckVerificationCodeErrorType,
    create_registration_session_response,
    get_registration_session_metadata_response,
    send_verification_code_response,
    check_verification_code_response,
};

/// Convert Entra ID errors to appropriate gRPC error responses
fn entra_error_to_registration_error(err: crate::auth::entra::Error) -> CreateRegistrationSessionError {
    match err {
        crate::auth::entra::Error::RateLimitExceeded(_) => CreateRegistrationSessionError {
            error_type: CreateRegistrationSessionErrorType::RateLimited as i32,
            may_retry: true,
            retry_after_seconds: 60, // Default 1 minute retry
        },
        crate::auth::entra::Error::PhoneNumberNotFound(_) => CreateRegistrationSessionError {
            error_type: CreateRegistrationSessionErrorType::IllegalPhoneNumber as i32,
            may_retry: false,
            retry_after_seconds: 0,
        },
        _ => CreateRegistrationSessionError {
            error_type: CreateRegistrationSessionErrorType::Unspecified as i32,
            may_retry: false,
            retry_after_seconds: 0,
        },
    }
}

/// Registration service implementation
pub struct RegistrationServer {
    entra_client: Arc<EntraIdClient>,
    session_store: SessionStore,
    session_timeout: std::time::Duration,
}

impl RegistrationServer {
    /// Create a new registration server instance
    pub fn new(entra_client: Arc<EntraIdClient>) -> Self {
        Self {
            entra_client,
            session_store: SessionStore::new(),
            session_timeout: std::time::Duration::from_secs(300), // Default 5 minutes
        }
    }

    /// Set the session timeout duration
    pub fn with_session_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.session_timeout = timeout;
        self
    }

    /// Generate a random 6-digit verification code
    fn generate_verification_code() -> String {
        let mut rng = rand::thread_rng();
        format!("{:06}", rng.gen::<u32>() % 1000000)
    }
}

#[tonic::async_trait]
impl RegistrationService for RegistrationServer {
    async fn create_session(
        &self,
        request: Request<CreateRegistrationSessionRequest>,
    ) -> Result<Response<CreateRegistrationSessionResponse>, Status> {
        // Get username and password from metadata
        let metadata = request.metadata();
        info!("📝 Received metadata: {:?}", metadata);
        
        let username = metadata.get("username")
            .ok_or_else(|| {
                error!("❌ Username missing from metadata");
                Status::invalid_argument("Username is required")
            })?
            .to_str()
            .map_err(|_| {
                error!("❌ Username is not valid UTF-8");
                Status::invalid_argument("Username must be valid UTF-8")
            })?
            .to_string();
        
        info!("👤 Got username: {}", username);
        
        let password = metadata.get("password")
            .ok_or_else(|| {
                error!("❌ Password missing from metadata");
                Status::invalid_argument("Password is required")
            })?
            .to_str()
            .map_err(|_| {
                error!("❌ Password is not valid UTF-8");
                Status::invalid_argument("Password must be valid UTF-8")
            })?
            .to_string();
        
        info!("🔑 Got password (length: {})", password.len());

        let req = request.into_inner();
        info!("➡️  Creating registration session for e164: {}", req.e164);

        // Validate credentials with Entra ID
        info!("🔍 Authenticating user with Entra ID...");
        match self.entra_client.authenticate_user(&username, &password).await {
            Ok(phone_number) => {
                info!("✅ Authentication successful, got phone number: {}", phone_number);
                let e164 = phone_number.parse::<u64>()
                    .map_err(|e| {
                        error!("❌ Failed to parse phone number '{}': {}", phone_number, e);
                        Status::internal("Failed to parse phone number")
                    })?;
                
                info!("📱 Parsed phone number as e164: {}", e164);
                let session_metadata = self.session_store.create_session(e164, self.session_timeout);
                info!("✅ Created session for e164: {}", e164);
                Ok(Response::new(CreateRegistrationSessionResponse {
                    response: Some(create_registration_session_response::Response::SessionMetadata(session_metadata)),
                }))
            }
            Err(err) => {
                error!("❌ Failed to validate credentials: {:?}", err);
                Ok(Response::new(CreateRegistrationSessionResponse {
                    response: Some(create_registration_session_response::Response::Error(
                        entra_error_to_registration_error(err),
                    )),
                }))
            }
        }
    }

    async fn get_session_metadata(
        &self,
        request: Request<GetRegistrationSessionMetadataRequest>,
    ) -> Result<Response<GetRegistrationSessionMetadataResponse>, Status> {
        let req = request.into_inner();
        info!("➡️  Getting session metadata");
        
        // Clean up expired sessions
        self.session_store.cleanup_expired();
        
        // Get and validate session
        if let Some(mut session) = self.session_store.get_session(&req.session_id) {
            if session.is_expired() {
                error!("❌ Session expired");
                return Err(Status::not_found("Session expired"));
            }
            
            // Update timing information
            session.update_timing();
            self.session_store.update_session(&req.session_id, session.clone());
            
            Ok(Response::new(GetRegistrationSessionMetadataResponse {
                response: Some(get_registration_session_metadata_response::Response::SessionMetadata(session.metadata)),
            }))
        } else {
            error!("❌ Session not found");
            Err(Status::not_found("Session not found"))
        }
    }

    async fn send_verification_code(
        &self,
        request: Request<SendVerificationCodeRequest>,
    ) -> Result<Response<SendVerificationCodeResponse>, Status> {
        let req = request.into_inner();
        info!("➡️  Sending verification code");
        
        // Clean up expired sessions
        self.session_store.cleanup_expired();
        
        // Get and validate session
        if let Some(mut session) = self.session_store.get_session(&req.session_id) {
            if session.is_expired() {
                error!("❌ Session expired");
                return Ok(Response::new(SendVerificationCodeResponse {
                    response: Some(send_verification_code_response::Response::Error(
                        SendVerificationCodeError {
                            error_type: SendVerificationCodeErrorType::SessionNotFound as i32,
                            may_retry: false,
                            retry_after_seconds: 0,
                        }
                    )),
                }));
            }
            
            // Update timing information
            session.update_timing();
            
            // Check if we can send a verification code
            match req.transport {
                0 => { // SMS
                    if !session.metadata.may_request_sms {
                        error!("❌ SMS rate limited");
                        return Ok(Response::new(SendVerificationCodeResponse {
                            response: Some(send_verification_code_response::Response::Error(
                                SendVerificationCodeError {
                                    error_type: SendVerificationCodeErrorType::RateLimited as i32,
                                    may_retry: true,
                                    retry_after_seconds: session.metadata.next_sms_seconds,
                                }
                            )),
                        }));
                    }
                    session.last_sms_at = Some(SystemTime::now());
                },
                1 => { // Voice
                    if !session.metadata.may_request_voice_call {
                        error!("❌ Voice call rate limited");
                        return Ok(Response::new(SendVerificationCodeResponse {
                            response: Some(send_verification_code_response::Response::Error(
                                SendVerificationCodeError {
                                    error_type: SendVerificationCodeErrorType::RateLimited as i32,
                                    may_retry: true,
                                    retry_after_seconds: session.metadata.next_voice_call_seconds,
                                }
                            )),
                        }));
                    }
                    session.last_voice_call_at = Some(SystemTime::now());
                },
                _ => {
                    error!("❌ Invalid transport type");
                    return Ok(Response::new(SendVerificationCodeResponse {
                        response: Some(send_verification_code_response::Response::Error(
                            SendVerificationCodeError {
                                error_type: SendVerificationCodeErrorType::TransportNotAllowed as i32,
                                may_retry: false,
                                retry_after_seconds: 0,
                            }
                        )),
                    }));
                }
            }
            
            // Generate and store verification code
            let code = Self::generate_verification_code();
            session.verification_code = Some(code.clone());
            session.metadata.may_check_code = true;
            session.metadata.next_code_check_seconds = 0;
            
            // TODO: Actually send the verification code via SMS or voice
            info!("✅ Generated verification code: {}", code);
            
            // Update session
            self.session_store.update_session(&req.session_id, session.clone());
            
            Ok(Response::new(SendVerificationCodeResponse {
                response: Some(send_verification_code_response::Response::SessionMetadata(session.metadata)),
            }))
        } else {
            error!("❌ Session not found");
            Ok(Response::new(SendVerificationCodeResponse {
                response: Some(send_verification_code_response::Response::Error(
                    SendVerificationCodeError {
                        error_type: SendVerificationCodeErrorType::SessionNotFound as i32,
                        may_retry: false,
                        retry_after_seconds: 0,
                    }
                )),
            }))
        }
    }

    async fn check_verification_code(
        &self,
        request: Request<CheckVerificationCodeRequest>,
    ) -> Result<Response<CheckVerificationCodeResponse>, Status> {
        let req = request.into_inner();
        info!("➡️  Checking verification code");
        
        // Clean up expired sessions
        self.session_store.cleanup_expired();
        
        // Get and validate session
        if let Some(mut session) = self.session_store.get_session(&req.session_id) {
            if session.is_expired() {
                error!("❌ Session expired");
                return Ok(Response::new(CheckVerificationCodeResponse {
                    response: Some(check_verification_code_response::Response::Error(
                        CheckVerificationCodeError {
                            error_type: CheckVerificationCodeErrorType::SessionNotFound as i32,
                            may_retry: false,
                            retry_after_seconds: 0,
                        }
                    )),
                }));
            }
            
            // Update timing information
            session.update_timing();
            
            // Check if we can verify a code
            if !session.metadata.may_check_code {
                error!("❌ Verification attempts exceeded");
                return Ok(Response::new(CheckVerificationCodeResponse {
                    response: Some(check_verification_code_response::Response::Error(
                        CheckVerificationCodeError {
                            error_type: CheckVerificationCodeErrorType::RateLimited as i32,
                            may_retry: true,
                            retry_after_seconds: session.metadata.next_code_check_seconds,
                        }
                    )),
                }));
            }
            
            // Verify the code
            if let Some(stored_code) = &session.verification_code {
                if req.verification_code == *stored_code {
                    session.metadata.verified = true;
                    info!("✅ Verification successful");
                    
                    // Update session
                    self.session_store.update_session(&req.session_id, session.clone());
                    
                    Ok(Response::new(CheckVerificationCodeResponse {
                        response: Some(check_verification_code_response::Response::SessionMetadata(session.metadata)),
                    }))
                } else {
                    session.verification_attempts += 1;
                    session.update_timing();
                    
                    // Update session
                    self.session_store.update_session(&req.session_id, session.clone());
                    
                    warn!("❌ Invalid verification code");
                    Ok(Response::new(CheckVerificationCodeResponse {
                        response: Some(check_verification_code_response::Response::SessionMetadata(session.metadata)),
                    }))
                }
            } else {
                error!("❌ No verification code found");
                Ok(Response::new(CheckVerificationCodeResponse {
                    response: Some(check_verification_code_response::Response::Error(
                        CheckVerificationCodeError {
                            error_type: CheckVerificationCodeErrorType::NoCodeSent as i32,
                            may_retry: true,
                            retry_after_seconds: 0,
                        }
                    )),
                }))
            }
        } else {
            error!("❌ Session not found");
            Ok(Response::new(CheckVerificationCodeResponse {
                response: Some(check_verification_code_response::Response::Error(
                    CheckVerificationCodeError {
                        error_type: CheckVerificationCodeErrorType::SessionNotFound as i32,
                        may_retry: false,
                        retry_after_seconds: 0,
                    }
                )),
            }))
        }
    }
}