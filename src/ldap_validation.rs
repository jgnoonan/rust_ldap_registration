//! LDAP validation service implementation.
//!
//! This module provides a gRPC service for validating LDAP credentials and retrieving
//! user information from LDAP directories. It is used to verify user existence and
//! retrieve associated phone numbers during the registration process.
//!
//! @author Joseph G Noonan
//! @copyright 2025

use tonic::{Request, Response, Status};
use tracing::{info, error, debug};

use crate::proto::org::signal::registration::ldap::rpc::{
    validate_credentials_response::Result as ValidateCredentialsResult,
    ValidateCredentialsResponse, ValidateCredentialsRequest, ValidateCredentialsError,
};

pub use crate::proto::org::signal::registration::ldap::rpc::ldap_validation_service_server::{
    LdapValidationService, LdapValidationServiceServer
};

use crate::auth::ldap::{LdapClient, Error as LdapError};

/// Server implementation for LDAP validation service.
///
/// Provides endpoints for validating user existence in LDAP and retrieving
/// associated user information such as phone numbers.
#[derive(Debug)]
pub struct LdapValidationServer {
    ldap_client: LdapClient,
}

impl LdapValidationServer {
    /// Creates a new LDAP validation server instance.
    ///
    /// # Arguments
    /// * `ldap_client` - Client for LDAP operations
    ///
    /// # Returns
    /// A new `LdapValidationServer` instance
    pub fn new(ldap_client: LdapClient) -> Self {
        Self { ldap_client }
    }
}

#[tonic::async_trait]
impl LdapValidationService for LdapValidationServer {
    /// Validates a user's LDAP credentials and retrieves their phone number.
    ///
    /// # Arguments
    /// * `request` - Contains the username and password to validate
    ///
    /// # Returns
    /// * Success: Response with user's phone number if authentication is successful
    /// * Error: Status with error details if validation fails
    async fn validate_credentials(
        &self,
        request: Request<ValidateCredentialsRequest>,
    ) -> Result<Response<ValidateCredentialsResponse>, Status> {
        let request = request.into_inner();
        
        info!("Received validation request for user: {}", request.user_id);
        debug!("Attempting LDAP authentication...");
        
        let result = self.ldap_client.authenticate_user(&request.user_id, &request.password).await;
        
        match result {
            Ok(phone_number) => {
                info!("Authentication successful for user: {}", request.user_id);
                Ok(Response::new(ValidateCredentialsResponse {
                    result: Some(ValidateCredentialsResult::PhoneNumber(phone_number)),
                }))
            }
            Err(err) => {
                let (error_type, message) = match err {
                    LdapError::UserNotFound(msg) => {
                        error!("User not found: {}", msg);
                        (1, msg)
                    }
                    LdapError::AuthenticationFailed => {
                        error!("Authentication failed");
                        (2, "Invalid credentials".to_string())
                    }
                    _ => {
                        error!("Server error: {:?}", err);
                        (3, format!("Server error: {}", err))
                    }
                };
                
                Ok(Response::new(ValidateCredentialsResponse {
                    result: Some(ValidateCredentialsResult::Error(ValidateCredentialsError {
                        error_type,
                        message,
                    })),
               }))
            }
        }
    }
}
