//! LDAP authentication and user validation.
//!
//! This module provides LDAP-based authentication and user validation services.
//! It handles connecting to LDAP servers, searching for users, and validating
//! credentials. The module is used by the registration service to verify user
//! identities and retrieve associated information like phone numbers.
//!
//! @author Joseph G Noonan
//! @copyright 2025
use ldap3::{
    Ldap, LdapConnAsync,
    result::{LdapError as Ldap3Error},
    Scope, SearchEntry,
};
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, error};
use tokio::sync::Mutex as TokioMutex;

/// Configuration for LDAP connection and operations.
#[derive(Debug, Clone)]
pub struct LdapConfig {
    /// LDAP server URL
    pub url: String,
    /// DN to bind with for initial connection
    pub bind_dn: String,
    /// Password for bind DN
    pub bind_password: String,
    /// Base DN for user searches
    pub base_dn: String,
    /// Attribute containing username
    pub username_attribute: String,
    /// Attribute containing phone number
    pub phone_number_attribute: String,
}

/// Errors that can occur during LDAP operations
#[derive(Error, Debug)]
pub enum Error {
    #[error("LDAP error: {0}")]
    Ldap(#[from] Ldap3Error),
    #[error("User not found: {0}")]
    UserNotFound(String),
    #[error("Phone number not found in attribute: {0}")]
    PhoneNumberNotFound(String),
    #[error("Phone number is empty")]
    PhoneNumberEmpty,
    #[error("Authentication failed")]
    AuthenticationFailed,
    #[error("Server error: {0}")]
    ServerError(String),
}

/// Client for LDAP authentication and user operations.
///
/// Provides methods for connecting to LDAP servers, searching for users,
/// and validating credentials. The client maintains a connection pool
/// and handles reconnection as needed.
#[derive(Debug, Clone)]
pub struct LdapClient {
    config: LdapConfig,
    pool: Arc<TokioMutex<Vec<Ldap>>>,
}

impl LdapClient {
    /// Escapes special characters in LDAP filter values
    fn escape_ldap_value(value: &str) -> String {
        value
            .replace('\\', "\\5c")
            .replace('*', "\\2a")
            .replace('(', "\\28")
            .replace(')', "\\29")
            .replace('\0', "\\00")
            .replace('/', "\\2f")
    }

    /// Creates a new LDAP client and establishes initial connection.
    ///
    /// # Arguments
    /// * `config` - LDAP configuration including server URL and credentials
    ///
    /// # Returns
    /// * `Result<Self>` - New client instance or error if connection fails
    pub async fn new(config: LdapConfig) -> Result<Self, Error> {
        let (conn, ldap) = LdapConnAsync::new(&config.url).await?;
        
        tokio::spawn(async move {
            conn.drive().await.ok();
        });
        
        let pool = Arc::new(TokioMutex::new(vec![ldap]));
        
        Ok(Self { config, pool })
    }
    
    /// Retrieves a connection from the pool or creates a new one if none are available.
    ///
    /// # Returns
    /// * `Result<Ldap>` - LDAP connection instance
    async fn get_connection(&self) -> Result<Ldap, Error> {
        let mut pool = self.pool.lock().await;
        if let Some(ldap) = pool.pop() {
            Ok(ldap)
        } else {
            let (conn, ldap) = LdapConnAsync::new(&self.config.url).await?;
            tokio::spawn(async move {
                conn.drive().await.ok();
            });
            Ok(ldap)
        }
    }
    
    /// Returns a connection to the pool after use.
    async fn return_connection(&self, ldap: Ldap) {
        let mut pool = self.pool.lock().await;
        pool.push(ldap);
    }
    
    /// Authenticates a user against LDAP.
    ///
    /// # Arguments
    /// * `username` - Username to authenticate
    /// * `password` - Password to check
    ///
    /// # Returns
    /// * `Result<String>` - User's phone number if authentication succeeds
    pub async fn authenticate_user(&self, username: &str, password: &str) -> Result<String, Error> {
        let ldap = self.get_connection().await?;
        
        // First find the user and get their DN
        let (user_dn, phone_number, ldap) = self.find_user(ldap, username).await?;
        
        // Return the connection to the pool
        self.return_connection(ldap).await;
        
        // Get a new connection for user authentication
        let mut ldap = self.get_connection().await?;
        
        // Bind with admin credentials
        ldap.simple_bind(&self.config.bind_dn, &self.config.bind_password)
            .await
            .map_err(|e| {
                error!("Admin bind failed: {:?}", e);
                Error::AuthenticationFailed
            })?.success()?;
        
        // Try to bind with user credentials
        ldap.simple_bind(&user_dn, password)
            .await
            .map_err(|e| {
                error!("User bind failed: {:?}", e);
                Error::AuthenticationFailed
            })?.success()?;

        debug!("User bind successful, returning phone number: {}", phone_number);
        
        // Return the connection to the pool after we're done using it
        self.return_connection(ldap).await;
        
        Ok(phone_number)
    }

    /// Searches for a user and retrieves their DN and phone number.
    ///
    /// # Arguments
    /// * `ldap` - LDAP connection instance
    /// * `username` - Username to search for
    ///
    /// # Returns
    /// * `Result<(String, String, Ldap)>` - User's DN, phone number, and LDAP connection instance
    async fn find_user(&self, mut ldap: Ldap, username: &str) -> Result<(String, String, Ldap), Error> {
        debug!("Input username: {}", username);
        
        // Extract username from email if email format is used
        let clean_username = if username.contains('@') {
            debug!("Email format detected, extracting username part");
            username.split('@').next().unwrap_or(username)
        } else {
            username
        };
        debug!("Clean username (without domain): {}", clean_username);
        
        // Escape special characters in the username for LDAP filter
        let escaped_username = Self::escape_ldap_value(clean_username);
        debug!("Escaped username: {}", escaped_username);
        
        // Construct LDAP filter
        let filter = format!("({}={})", self.config.username_attribute, escaped_username);
        debug!("LDAP search parameters:");
        debug!("  Base DN: {}", self.config.base_dn);
        debug!("  Username attribute: {}", self.config.username_attribute);
        debug!("  Filter: {}", filter);
        debug!("  Phone number attribute: {}", self.config.phone_number_attribute);
        
        let (mut entries, result) = ldap.search(
            &self.config.base_dn,
            Scope::Subtree,
            &filter,
            vec![&self.config.phone_number_attribute],
        ).await.map_err(|e| {
            error!("LDAP search failed: {:?}", e);
            Error::ServerError(e.to_string())
        })?.success()?;
        
        debug!("LDAP search result: {:?}", result);
        debug!("Number of entries found: {}", entries.len());
        
        if entries.is_empty() {
            error!("No user found with username: {}", username);
            return Err(Error::UserNotFound(username.to_string()));
        }
        
        let entry = SearchEntry::construct(entries.remove(0));
        let user_dn = entry.dn;
        debug!("Found user entry with DN: {}", user_dn);
        
        // Extract phone number from the attributes
        let phone_number = entry.attrs
            .get(&self.config.phone_number_attribute)
            .and_then(|vals: &Vec<String>| vals.first().map(|v| v.to_string()))  
            .ok_or_else(|| {
                error!("Phone number attribute not found");
                Error::PhoneNumberNotFound(self.config.phone_number_attribute.clone())
            })?;
        
        if phone_number.trim().is_empty() {
            error!("Phone number is empty for user");
            return Err(Error::PhoneNumberEmpty);
        }
        
        debug!("Found phone number: {}", phone_number);
        Ok((user_dn, phone_number, ldap))
   }
}
