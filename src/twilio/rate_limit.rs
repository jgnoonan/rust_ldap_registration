use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::Mutex;
use tracing::warn;

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub max_attempts: u32,
    pub window_secs: u64,
}

#[derive(Debug)]
struct RateLimitEntry {
    attempts: u32,
    window_start: SystemTime,
}

pub struct RateLimiter {
    config: RateLimitConfig,
    attempts: Arc<Mutex<HashMap<String, RateLimitEntry>>>,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            attempts: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    pub async fn check_rate_limit(&self, key: &str) -> bool {
        let mut attempts = self.attempts.lock().await;
        let now = SystemTime::now();
        
        // Clean up old entries
        attempts.retain(|_, entry| {
            now.duration_since(entry.window_start)
                .map(|duration| duration.as_secs() < self.config.window_secs)
                .unwrap_or(false)
        });
        
        // Check and update rate limit
        if let Some(entry) = attempts.get_mut(key) {
            if entry.attempts >= self.config.max_attempts {
                warn!("Rate limit exceeded for key: {}", key);
                return false;
            }
            
            if let Ok(duration) = now.duration_since(entry.window_start) {
                if duration.as_secs() >= self.config.window_secs {
                    entry.attempts = 1;
                    entry.window_start = now;
                } else {
                    entry.attempts += 1;
                }
            }
        } else {
            attempts.insert(
                key.to_string(),
                RateLimitEntry {
                    attempts: 1,
                    window_start: now,
                },
            );
        }
        
        true
    }
    
    pub async fn reset_rate_limit(&self, key: &str) {
        let mut attempts = self.attempts.lock().await;
        attempts.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_rate_limit() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_attempts: 3,
            window_secs: 60,
        });
        
        let key = "test_key";
        
        // First three attempts should succeed
        assert!(limiter.check_rate_limit(key).await);
        assert!(limiter.check_rate_limit(key).await);
        assert!(limiter.check_rate_limit(key).await);
        
        // Fourth attempt should fail
        assert!(!limiter.check_rate_limit(key).await);
        
        // Reset should allow new attempts
        limiter.reset_rate_limit(key).await;
        assert!(limiter.check_rate_limit(key).await);
    }
}
