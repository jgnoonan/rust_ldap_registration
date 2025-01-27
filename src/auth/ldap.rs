/// LDAP Authentication Module
///
/// Provides LDAP authentication and user information retrieval functionality for the
/// Signal Registration Service. This module handles connection pooling, user authentication,
/// and phone number retrieval from LDAP records.
///
/// # Features
/// - Connection pooling for improved performance
/// - Secure LDAP authentication
/// - Phone number attribute retrieval
/// - Configurable retry mechanism
///
/// # Copyright
/// Copyright (c) 2025 Signal Messenger, LLC
/// All rights reserved.
///
/// # License
/// Licensed under the AGPLv3 license.

use ldap3::{Ldap, LdapConnAsync, Scope, SearchEntry};
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info};
use phonenumber;

/// Configuration for LDAP connection and authentication
#[derive(Debug, Clone)]
pub struct LdapConfig {
    /// LDAP server URL (e.g., "ldap://localhost:389")
    pub url: String,
    /// DN used to bind to LDAP server
    pub bind_dn: String,
    /// Password for binding to LDAP server
    pub bind_password: String,
    /// Base DN for LDAP searches
    pub search_base: String,
    /// Filter template for finding users (e.g., "(uid=%s)")
    pub search_filter: String,
    /// LDAP attribute containing the phone number
    pub phone_number_attribute: String,
    /// Connection pool size
    pub connection_pool_size: usize,
    /// Timeout in seconds
    pub timeout_secs: u64,
}

/// Error types for LDAP operations
#[derive(Debug, thiserror::Error)]
pub enum LdapError {
    #[error("LDAP connection failed: {0}")]
    ConnectionFailed(String),
    
    #[error("LDAP bind failed: {0}")]
    BindFailed(String),
    
    #[error("LDAP search failed: {0}")]
    SearchFailed(String),
    
    #[error("Invalid phone number: {0}")]
    InvalidPhoneNumber(String),
    
    #[error("User not found")]
    UserNotFound,
    
    #[error("Phone number attribute missing")]
    PhoneNumberMissing,
}

/// Client for LDAP operations with connection pooling
#[derive(Debug)]
pub struct LdapClient {
    config: LdapConfig,
    pool: Arc<Mutex<Vec<Ldap>>>,
}

impl LdapClient {
    /// Creates a new LDAP client with the specified configuration
    pub async fn new(config: LdapConfig) -> Result<Self, LdapError> {
        info!("Creating LDAP client with URL: {}", config.url);
        info!("Using bind DN: {}", config.bind_dn);
        
        // Try to establish a single connection first
        info!("Establishing TCP connection to LDAP server...");
        let (conn, mut ldap) = LdapConnAsync::new(&config.url)
            .await
            .map_err(|e| {
                error!("Failed to establish LDAP connection: {}", e);
                LdapError::ConnectionFailed(e.to_string())
            })?;
            
        info!("TCP connection established successfully");
            
        // Keep the connection alive in a separate task
        tokio::spawn(async move {
            info!("Starting connection handler task");
            let _ = conn;
        });
        
        info!("Attempting LDAP bind with DN: {} (password length: {})", 
              config.bind_dn, config.bind_password.len());
        
        // Simple bind without timeout
        let bind_result = ldap.simple_bind(&config.bind_dn, &config.bind_password)
            .await
            .map_err(|e| {
                error!("LDAP bind request failed: {} (DN: {})", e, config.bind_dn);
                LdapError::BindFailed(e.to_string())
            })?;
        
        // Log the raw bind result for debugging
        info!("Raw bind result: {:?}", bind_result);
        
        // Check bind success and get detailed result
        let result = bind_result.success().map_err(|e| {
            error!("LDAP bind failed with error: {}", e);
            error!("Error details: {:?}", e);
            LdapError::BindFailed(format!("Bind failed: {}", e))
        })?;
        
        info!("LDAP bind successful with result: {:?}", result);
            
        // Create a pool with just this one connection for now
        let mut pool = Vec::with_capacity(1);
        pool.push(ldap);
        
        info!("LDAP client initialization complete");
        Ok(Self {
            config,
            pool: Arc::new(Mutex::new(pool)),
        })
    }
    
    /// Validates and formats a phone number according to E.164 format
    fn validate_phone_number(phone: &str) -> Result<String, LdapError> {
        // Try to parse the phone number
        let phone_number = phonenumber::parse(None, phone)
            .map_err(|e| LdapError::InvalidPhoneNumber(e.to_string()))?;
            
        // Ensure the phone number is valid
        if !phonenumber::is_valid(&phone_number) {
            return Err(LdapError::InvalidPhoneNumber("Invalid phone number format".to_string()));
        }
        
        // Format to E.164
        Ok(phone_number.format().mode(phonenumber::Mode::E164).to_string())
    }

    /// Authenticates a user and retrieves their phone number
    pub async fn authenticate_and_get_phone(&self, username: &str, password: &str) -> Result<String, LdapError> {
        let mut pool = self.pool.lock().await;
        let mut ldap = pool.pop().ok_or_else(|| LdapError::ConnectionFailed("No available connections".to_string()))?;
        
        let result = async {
            let search_filter = self.config.search_filter.replace("{}", username);
            
            let result = ldap.search(
                &self.config.search_base,
                Scope::Subtree,
                &search_filter,
                vec![&self.config.phone_number_attribute],
            ).await.map_err(|e| LdapError::SearchFailed(e.to_string()))?;
            
            let entry = result.0.first()
                .ok_or(LdapError::UserNotFound)?;
            
            let entry = SearchEntry::construct(entry.clone());
            let phone = entry.attrs.get(&self.config.phone_number_attribute)
                .and_then(|attrs| attrs.first())
                .ok_or(LdapError::PhoneNumberMissing)?;
                
            // Validate and format the phone number
            let phone_number = Self::validate_phone_number(phone)?;
            
            // Verify the password
            let (conn, mut ldap) = LdapConnAsync::new(&self.config.url)
                .await
                .map_err(|e| LdapError::ConnectionFailed(e.to_string()))?;
                
            // Spawn the connection handler
            tokio::spawn(async move {
                let _ = conn;  // LdapConnAsync is not a future, just drop it
            });
            
            ldap.simple_bind(&entry.dn, password)
                .await
                .map_err(|e| LdapError::BindFailed(e.to_string()))?
                .success()
                .map_err(|e| LdapError::BindFailed(e.to_string()))?;
                
            info!("Successfully authenticated user: {}", username);
            Ok(phone_number)
        }.await;
        
        // Return the connection to the pool
        pool.push(ldap);
        
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_ldap_authentication() {
        // Add tests here
    }
}
