/// Signal Registration Service Library
///
/// This library provides the core functionality for the Signal Registration Service,
/// including LDAP authentication, Twilio verification, and DynamoDB storage.
///
/// # Features
/// - LDAP authentication and user management
/// - Twilio SMS and voice verification
/// - DynamoDB data persistence
/// - gRPC service interface
/// - Rate limiting and security
///
/// # Modules
/// - `auth`: LDAP authentication and user management
/// - `twilio`: Phone number verification via SMS and voice
/// - `db`: DynamoDB storage and data management
/// - `grpc`: gRPC service implementation
/// - `config`: Configuration management
/// - `ldap_validation`: LDAP validation service
///
/// # Example
/// ```no_run
/// use registration_service::{
///     auth::ldap::LdapClient,
///     twilio::TwilioClient,
///     db::dynamodb::DynamoDbClient,
///     config::Settings,
/// };
///
/// async fn setup_service() {
///     let settings = Settings::new().expect("Failed to load configuration");
///     let ldap_client = LdapClient::new(settings.ldap).await.expect("Failed to create LDAP client");
///     let twilio_client = TwilioClient::new(settings.twilio).expect("Failed to create Twilio client");
///     let dynamodb_client = DynamoDbClient::new(settings.dynamodb).await.expect("Failed to create DynamoDB client");
/// }
/// ```
///
/// # Copyright
/// Copyright (c) 2025 Signal Messenger, LLC
/// All rights reserved.
///
/// # License
/// Licensed under the AGPLv3 license.

pub mod auth;
pub mod twilio;
pub mod db;
pub mod grpc;
pub mod config;
pub mod ldap_validation;

/// Generated protocol buffer code
pub mod proto {
    pub mod registration {
        tonic::include_proto!("org.signal.registration");
    }
    pub mod org {
        pub mod signal {
            pub mod registration {
                pub mod ldap {
                    pub mod rpc {
                        tonic::include_proto!("org.signal.registration.ldap.rpc");
                    }
                }
            }
        }
    }
}