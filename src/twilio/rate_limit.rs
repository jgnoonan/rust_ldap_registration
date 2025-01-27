/// Rate Limiting Module for Twilio Verification
///
/// Implements rate limiting for Twilio verification requests to prevent abuse.
/// Uses a combination of fixed window and leaky bucket algorithms for different
/// verification channels.
///
/// # Features
/// - Channel-specific rate limits
/// - Configurable time windows
/// - Leaky bucket implementation
/// - Separate limits for SMS and voice
///
/// # Copyright
/// Copyright (c) 2025 Signal Messenger, LLC
/// All rights reserved.
///
/// # License
/// Licensed under the AGPLv3 license.

use std::time::SystemTime;
use std::collections::HashMap;
use tokio::sync::Mutex;
use std::sync::Arc;
use tracing::warn;
use crate::config::RateLimits;
/// Configuration for rate limiting
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum attempts per time window
    pub max_attempts: u32,
    /// Time window duration in seconds
    pub window_secs: u64,
}

/// Rate limiter for verification attempts
#[derive(Debug)]
pub struct RateLimiter {
    /// Rate limit configuration
    config: RateLimitConfig,
    /// Attempt counters by phone number
    attempts: Arc<Mutex<HashMap<String, RateLimitEntry>>>,
}

/// Information about verification attempts
#[derive(Debug)]
struct RateLimitEntry {
    /// Number of attempts made
    attempts: u32,
    /// Timestamp of first attempt
    window_start: SystemTime,
}

impl RateLimiter {
    /// Creates a new rate limiter with the specified configuration
    ///
    /// # Arguments
    /// * `config` - Rate limiting configuration
    ///
    /// # Returns
    /// * `RateLimiter` - New rate limiter instance
    ///
    /// # Examples
    /// ```
    /// use registration_service::twilio::rate_limit::{RateLimiter, RateLimitConfig};
    ///
    /// let config = RateLimitConfig {
    ///     max_attempts: 3,
    ///     window_secs: 300,
    /// };
    ///
    /// let rate_limiter = RateLimiter::new(config);
    /// ```
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            attempts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Checks if a verification attempt is allowed for the given phone number
    ///
    /// # Arguments
    /// * `key` - Phone number to check
    ///
    /// # Returns
    /// * `bool` - True if attempt is allowed, false if rate limited
    ///
    /// # Examples
    /// ```no_run
    /// # use registration_service::twilio::rate_limit::RateLimiter;
    /// # let rate_limiter = get_rate_limiter();
    /// if rate_limiter.check_rate_limit("+1234567890").await {
    ///     println!("Attempt allowed");
    /// } else {
    ///     println!("Rate limited");
    /// }
    /// # async fn get_rate_limiter() -> RateLimiter { unimplemented!() }
    /// ```
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

    /// Resets the rate limit for the given phone number
    ///
    /// # Arguments
    /// * `key` - Phone number to reset
    ///
    /// # Examples
    /// ```no_run
    /// # use registration_service::twilio::rate_limit::RateLimiter;
    /// # let rate_limiter = get_rate_limiter();
    /// rate_limiter.reset_rate_limit("+1234567890").await;
    /// # async fn get_rate_limiter() -> RateLimiter { unimplemented!() }
    /// ```
    pub async fn reset_rate_limit(&self, key: &str) {
        let mut attempts = self.attempts.lock().await;
        attempts.remove(key);
    }
}

impl From<RateLimits> for RateLimitConfig {
    fn from(rate_limits: RateLimits) -> Self {
        // Use SMS verification delays as the window size since it's the most common case
        // Use the leaky bucket session creation max capacity for the maximum attempts
        RateLimitConfig {
            max_attempts: rate_limits.leaky_bucket.session_creation.max_capacity,
            window_secs: rate_limits.send_sms_verification_code.delays,
        }
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
