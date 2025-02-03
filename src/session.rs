use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};

use uuid::Uuid;

use crate::proto::RegistrationSessionMetadata;

/// Session data including verification attempts and timing information
#[derive(Clone, Debug)]
pub struct SessionData {
    pub metadata: RegistrationSessionMetadata,
    pub created_at: SystemTime,
    pub last_sms_at: Option<SystemTime>,
    pub last_voice_call_at: Option<SystemTime>,
    pub verification_attempts: u32,
    pub verification_code: Option<String>,
}

impl SessionData {
    pub fn new(metadata: RegistrationSessionMetadata) -> Self {
        Self {
            metadata,
            created_at: SystemTime::now(),
            last_sms_at: None,
            last_voice_call_at: None,
            verification_attempts: 0,
            verification_code: None,
        }
    }

    /// Check if the session has expired
    pub fn is_expired(&self) -> bool {
        SystemTime::now()
            .duration_since(self.created_at)
            .map(|elapsed| elapsed.as_secs() >= self.metadata.expiration_seconds)
            .unwrap_or(true)
    }

    /// Update session metadata with current timing information
    pub fn update_timing(&mut self) {
        let now = SystemTime::now();
        
        // Update SMS timing
        if let Some(last_sms) = self.last_sms_at {
            if let Ok(elapsed) = now.duration_since(last_sms) {
                self.metadata.may_request_sms = elapsed.as_secs() >= 60; // Allow SMS every 60 seconds
                self.metadata.next_sms_seconds = if self.metadata.may_request_sms {
                    0
                } else {
                    60 - elapsed.as_secs()
                };
            }
        }

        // Update voice call timing
        if let Some(last_call) = self.last_voice_call_at {
            if let Ok(elapsed) = now.duration_since(last_call) {
                self.metadata.may_request_voice_call = elapsed.as_secs() >= 300; // Allow voice calls every 5 minutes
                self.metadata.next_voice_call_seconds = if self.metadata.may_request_voice_call {
                    0
                } else {
                    300 - elapsed.as_secs()
                };
            }
        }

        // Update code check timing
        if self.verification_attempts > 0 {
            self.metadata.may_check_code = self.verification_attempts < 3; // Allow up to 3 attempts
            self.metadata.next_code_check_seconds = if self.metadata.may_check_code {
                0
            } else {
                300 // 5 minute lockout after 3 failed attempts
            };
        }
    }
}

/// Session store for managing registration sessions
#[derive(Clone)]
pub struct SessionStore {
    sessions: Arc<RwLock<HashMap<Vec<u8>, SessionData>>>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new session
    pub fn create_session(&self, e164: u64, timeout: Duration) -> RegistrationSessionMetadata {
        let session_id = Uuid::new_v4().as_bytes().to_vec();
        let metadata = RegistrationSessionMetadata {
            session_id: session_id.clone(),
            verified: false,
            e164,
            may_request_sms: true,
            next_sms_seconds: 0,
            may_request_voice_call: true,
            next_voice_call_seconds: 0,
            may_check_code: false,
            next_code_check_seconds: 0,
            expiration_seconds: timeout.as_secs() as u64,
        };
        
        let session = SessionData::new(metadata.clone());
        self.sessions.write().unwrap().insert(session_id, session);
        
        metadata
    }

    /// Get session data by session ID
    pub fn get_session(&self, session_id: &[u8]) -> Option<SessionData> {
        self.sessions.read().unwrap().get(session_id).cloned()
    }

    /// Update session data
    pub fn update_session(&self, session_id: &[u8], data: SessionData) -> bool {
        if let Some(session) = self.sessions.write().unwrap().get_mut(session_id) {
            *session = data;
            true
        } else {
            false
        }
    }

    /// Remove expired sessions
    pub fn cleanup_expired(&self) {
        self.sessions.write().unwrap().retain(|_, session| !session.is_expired());
    }
}
