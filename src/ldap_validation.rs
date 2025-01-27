use tonic::{Request, Response, Status};
use crate::auth::ldap::{LdapClient, Error as LdapError};
use tracing::{info, error, debug};

pub use crate::proto::org::signal::registration::ldap::rpc::{
    ValidateCredentialsRequest,
    ValidateCredentialsResponse,
    ValidateCredentialsError,
    ValidateCredentialsErrorType,
    ldap_validation_service_server::{LdapValidationService, LdapValidationServiceServer},
};

#[derive(Debug)]
pub struct LdapValidationServer {
    ldap_client: LdapClient,
}

impl LdapValidationServer {
    pub fn new(ldap_client: LdapClient) -> Self {
        Self { ldap_client }
    }
}

#[tonic::async_trait]
impl LdapValidationService for LdapValidationServer {
    async fn validate_credentials(
        &self,
        request: Request<ValidateCredentialsRequest>,
    ) -> Result<Response<ValidateCredentialsResponse>, Status> {
        let request = request.into_inner();
        info!("Received validation request for user: {}", request.user_id);
        debug!("Attempting LDAP authentication...");
        
        match self.ldap_client.authenticate_user(&request.user_id, &request.password).await {
            Ok(phone_number) => {
                info!("Authentication successful for user: {}", request.user_id);
                debug!("Retrieved phone number: {}", phone_number);
                
                let response = ValidateCredentialsResponse {
                    result: Some(crate::proto::org::signal::registration::ldap::rpc::validate_credentials_response::Result::PhoneNumber(
                        phone_number,
                    )),
                };
                Ok(Response::new(response))
            }
            Err(error) => {
                let error_type = match error {
                    LdapError::AuthenticationFailed => {
                        error!("Authentication failed for user: {}", request.user_id);
                        ValidateCredentialsErrorType::InvalidCredentials
                    }
                    LdapError::UserNotFound => {
                        error!("User not found: {}", request.user_id);
                        ValidateCredentialsErrorType::UserNotFound
                    }
                    LdapError::PhoneNumberNotFound(_) | LdapError::PhoneNumberEmpty => {
                        error!("Phone number not found for user: {}", request.user_id);
                        ValidateCredentialsErrorType::PhoneNumberNotFound
                    }
                    _ => {
                        error!("Server error during authentication: {:?}", error);
                        ValidateCredentialsErrorType::ServerError
                    }
                };
                
                let error = ValidateCredentialsError {
                    error_type: error_type as i32,
                    message: error.to_string(),
                };
                
                let response = ValidateCredentialsResponse {
                    result: Some(crate::proto::org::signal::registration::ldap::rpc::validate_credentials_response::Result::Error(
                        error,
                    )),
                };
                Ok(Response::new(response))
            }
        }
    }
}
