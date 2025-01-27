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
use tracing::{info, error, debug};
use std::time::{SystemTime, Duration};
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Session data for registration process
#[derive(Debug)]
struct Session {
    username: String,
    phone_number: String,
    verified: bool,
    created_at: SystemTime,
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
            Error::UserNotFound => 
                Status::not_found("User not found"),
            Error::AuthenticationFailed => 
                Status::unauthenticated("Authentication failed"),
            Error::ServerError => 
                Status::internal("Server error"),
        }
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

    pub async fn cleanup_expired_sessions(&self) {
        let mut sessions = self.sessions.lock().await;
        sessions.retain(|_, session| {
            SystemTime::now()
                .duration_since(session.created_at)
                .unwrap_or_default() <= self.session_timeout
        });
    }
}