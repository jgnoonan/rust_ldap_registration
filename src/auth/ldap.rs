use anyhow::Result;
use ldap3::{LdapConnAsync, Scope, SearchEntry};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info};

#[derive(Clone)]
pub struct LdapConfig {
    pub url: String,
    pub bind_dn: String,
    pub bind_password: String,
    pub search_base: String,
    pub search_filter: String,
    pub phone_number_attribute: String,
    pub connection_pool_size: usize,
    pub timeout_secs: u64,
}

pub struct LdapClient {
    config: LdapConfig,
    pool: Arc<Mutex<Vec<LdapConnAsync>>>,
}

impl LdapClient {
    pub async fn new(config: LdapConfig) -> Result<Self> {
        let mut pool = Vec::with_capacity(config.connection_pool_size);
        
        for _ in 0..config.connection_pool_size {
            let (conn, mut ldap) = LdapConnAsync::new(&config.url).await?;
            tokio::spawn(conn);
            
            ldap.simple_bind(&config.bind_dn, &config.bind_password)
                .await?
                .success()?;
                
            pool.push(ldap);
        }
        
        Ok(Self {
            config,
            pool: Arc::new(Mutex::new(pool)),
        })
    }
    
    pub async fn authenticate_and_get_phone(&self, username: &str, password: &str) -> Result<Option<String>> {
        let mut pool = self.pool.lock().await;
        if let Some(ldap) = pool.pop() {
            let search_filter = self.config.search_filter.replace("{}", username);
            
            let search = ldap.search(
                &self.config.search_base,
                Scope::Subtree,
                &search_filter,
                vec![&self.config.phone_number_attribute],
            ).await?;
            
            if let Some(entry) = search.0.first() {
                let entry = SearchEntry::construct(entry.clone());
                if let Some(phone) = entry.attrs.get(&self.config.phone_number_attribute) {
                    if let Some(phone_number) = phone.first() {
                        // Verify the password
                        let (conn, mut bind_ldap) = LdapConnAsync::new(&self.config.url).await?;
                        tokio::spawn(conn);
                        
                        match bind_ldap.simple_bind(&entry.dn, password).await {
                            Ok(result) => {
                                if result.success().is_ok() {
                                    info!("Successfully authenticated user: {}", username);
                                    pool.push(ldap);
                                    return Ok(Some(phone_number.to_string()));
                                }
                            }
                            Err(e) => {
                                error!("Failed to bind with user credentials: {}", e);
                            }
                        }
                    }
                }
            }
            
            pool.push(ldap);
        }
        
        Ok(None)
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
