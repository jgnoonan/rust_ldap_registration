//! gRPC server implementation for the Signal Registration Service.
//!
//! This module implements the gRPC service endpoints defined in the proto files,
//! handling user registration and LDAP validation requests. It manages user sessions,
//! rate limiting, and coordinates between various backend services (LDAP, Twilio, DynamoDB).
//!
//! @author Joseph G Noonan
//! @copyright 2025
use tonic::{Request, Response, Status};
use crate::auth::ldap::{LdapClient, Error};
use crate::twilio::TwilioClient;
use crate::db::dynamodb::DynamoDbClient;
use crate::twilio::rate_limit::RateLimiter;
use crate::proto::registration::{
    StartRegistrationRequest,
    StartRegistrationResponse,
    VerifyCodeRequest,
    VerifyCodeResponse,
    CompleteRegistrationRequest,
    CompleteRegistrationResponse,
    registration_service_server::RegistrationService,
};
use tracing::{error, debug};
use std::time::{SystemTime, Duration};
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Represents a user registration session with associated state and timing information.
#[derive(Debug)]
struct Session {
    /// Username associated with the session
    username: String,
    /// Phone number being verified
    phone_number: String,
    /// Timestamp when the session was created
    created_at: SystemTime,
    /// Whether the session has been verified
    verified: bool,
}

/// Maps LDAP errors to gRPC status codes
impl From<Error> for Status {
    fn from(error: Error) -> Self {
        match error {
            Error::Ldap(e) => Status::internal(format!("LDAP error: {}", e)),
            Error::PhoneNumberNotFound(attr) => 
                Status::not_found(format!("Phone number not found in attribute: {}", attr)),
            Error::PhoneNumberEmpty => 
                Status::invalid_argument("Phone number is empty"),
            Error::UserNotFound(msg) => 
                Status::not_found(format!("User not found: {}", msg)),
            Error::AuthenticationFailed => 
                Status::unauthenticated("Authentication failed"),
            Error::ServerError(msg) => 
                Status::internal(format!("Server error: {}", msg)),
        }
    }
}

/// Main server implementation for the registration service.
///
/// Handles all gRPC endpoints related to user registration, including:
/// - Starting registration process
/// - Verifying phone numbers
/// - Completing registration
///
/// The server maintains session state and coordinates between LDAP authentication,
/// Twilio phone verification, and DynamoDB persistence.
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
    /// Starts a new registration process for a user.
    ///
    /// # Arguments
    /// * `request` - Contains the username to validate
    ///
    /// # Returns
    /// * Success: Response with session token for subsequent requests
    /// * Error: Status with error details if validation fails
    ///
    /// # Flow
    /// 1. Validates username exists in LDAP
    /// 2. Creates new session
    /// 3. Returns session token to client
    async fn start_registration(
        &self,
        request: Request<StartRegistrationRequest>,
    ) -> Result<Response<StartRegistrationResponse>, Status> {
        let req = request.into_inner();
        
        debug!("Received validation request for user: {}", req.username);
        debug!("Attempting LDAP authentication...");
        
        // Authenticate with LDAP and get phone number
        let phone_number = self.ldap_client
            .authenticate_user(&req.username, &req.password)
            .await
            .map_err(|e: Error| {
                error!("LDAP authentication failed: {}", e);
                Status::from(e)
           })?;
        
        debug!("LDAP authentication successful, sending verification code...");
        
        // Check rate limit
        if !self.rate_limiter.check_rate_limit(&phone_number).await {
            return Err(Status::resource_exhausted("Too many verification attempts"));
        }
        
        // Start Twilio verification
        self.twilio_client
            .send_verification_code(&phone_number, match req.channel.as_str() {
                "sms" => crate::twilio::VerificationChannel::Sms,
                "voice" => crate::twilio::VerificationChannel::Voice,
                _ => return Err(Status::invalid_argument("Invalid channel. Must be 'sms' or 'voice'")),
            })
            .await
            .map_err(|e| {
                error!("Failed to send verification code: {}", e);
                Status::internal(format!("Failed to send verification code: {}", e))
            })?;
        
        debug!("Verification code sent successfully");
        
        // Create session
        let session_id = Uuid::new_v4().to_string();
        let session = Session {
            username: req.username.clone(),
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

    /// Verifies a phone number for registration.
    ///
    /// # Arguments
    /// * `request` - Contains session token and verification code
    ///
    /// # Returns
    /// * Success: Response indicating verification code was sent
    /// * Error: Status with error details if verification fails
    ///
    /// # Flow
    /// 1. Validates session exists and is valid
    /// 2. Verifies code with Twilio
    /// 3. Updates session state
    async fn verify_code(
        &self,
        request: Request<VerifyCodeRequest>,
    ) -> Result<Response<VerifyCodeResponse>, Status> {
        let req = request.into_inner();
        
        debug!("Received verification code for session: {}", req.session_id);
        
        // Get session
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .get_mut(&req.session_id)
            .ok_or_else(|| {
                error!("Session not found");
                Status::not_found("Session not found")
            })?;
            
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
            .map_err(|e| {
                error!("Failed to verify code: {}", e);
                Status::internal(format!("Failed to verify code: {}", e))
            })?;
            
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

    /// Completes the registration process.
    ///
    /// # Arguments
    /// * `request` - Contains session token and verification code
    ///
    /// # Returns
    /// * Success: Response indicating successful registration
    /// * Error: Status with error details if verification fails
    ///
    /// # Flow
    /// 1. Validates session exists and is valid
    /// 2. Verifies code with Twilio
    /// 3. Stores registration in DynamoDB
    /// 4. Cleans up session
    async fn complete_registration(
        &self,
        request: Request<CompleteRegistrationRequest>,
    ) -> Result<Response<CompleteRegistrationResponse>, Status> {
        let req = request.into_inner();
        
        debug!("Received complete registration request for session: {}", req.session_id);
        
        // Get and remove session
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .remove(&req.session_id)
            .ok_or_else(|| {
                error!("Session not found");
                Status::not_found("Session not found")
            })?;
            
        // Check if session is verified
        if !session.verified {
            return Ok(Response::new(CompleteRegistrationResponse {
                success: false,
                message: "Phone number not verified".to_string(),
            }));
        }
        
        // Save registration
        match self.dynamodb_client.save_registration(
            &session.username,
            &session.phone_number,
            &format!("{}", req.registration_id),
        ).await {
            Ok(_) => Ok(Response::new(CompleteRegistrationResponse {
                success: true,
                message: "Registration completed successfully".to_string(),
            })),
            Err(e) => {
                error!("Failed to save registration: {}", e);
                Ok(Response::new(CompleteRegistrationResponse {
                    success: false,
                    message: format!("Failed to save registration: {}", e),
                }))
            }
        }
    }
}

impl RegistrationServer {
    /// Creates a new instance of the registration server.
    ///
    /// # Arguments
    /// * `ldap_client` - Client for LDAP authentication and user validation
    /// * `twilio_client` - Client for phone number verification via Twilio
    /// * `dynamodb_client` - Client for persistent storage in DynamoDB
    /// * `rate_limiter` - Rate limiter to prevent abuse
    /// * `session_timeout_secs` - Session timeout in seconds
    ///
    /// # Returns
    /// A new `RegistrationServer` instance configured with the provided clients
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

    /// Removes expired sessions from the session store.
    ///
    /// This is called periodically to prevent memory leaks from abandoned sessions.
    pub async fn cleanup_expired_sessions(&self) {
        let mut sessions = self.sessions.lock().await;
        sessions.retain(|_, session| {
            SystemTime::now()
                .duration_since(session.created_at)
                .unwrap_or_default() <= self.session_timeout
        });
    }
}