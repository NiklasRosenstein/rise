use base64ct::{Base64UrlUnpadded, Encoding};
use moka::sync::Cache;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;

/// OAuth2 state data stored temporarily during the PKCE flow
#[derive(Debug, Clone)]
pub struct OAuth2State {
    pub code_verifier: String,
    pub redirect_url: Option<String>,
    /// Project name for ingress authentication flow
    pub project_name: Option<String>,
    /// Custom domain callback URL for custom domain auth routing
    /// When set, after IdP callback completes on the main domain,
    /// we redirect to this URL on the custom domain to set cookies there
    pub custom_domain_callback_url: Option<String>,
}

/// Completed auth session data for custom domain token exchange
/// After IdP callback on main domain, this data is stored temporarily
/// and retrieved by the custom domain to set cookies
#[derive(Debug, Clone)]
pub struct CompletedAuthSession {
    /// The signed Rise JWT to set as cookie
    pub rise_jwt: String,
    /// Max age for the cookie in seconds
    pub max_age: u64,
    /// Final redirect URL after setting cookie
    pub redirect_url: String,
    /// Project name for display
    pub project_name: String,
}

/// Trait for storing and retrieving OAuth2 state tokens
pub trait TokenStore: Send + Sync {
    /// Save OAuth2 state with the given state token as the key
    fn save(&self, state: String, data: OAuth2State);

    /// Retrieve OAuth2 state by state token
    fn get(&self, state: &str) -> Option<OAuth2State>;

    /// Save completed auth session for custom domain token exchange
    fn save_completed_session(&self, token: String, session: CompletedAuthSession);

    /// Retrieve and consume completed auth session by token
    fn get_completed_session(&self, token: &str) -> Option<CompletedAuthSession>;
}

/// In-memory implementation of TokenStore using Moka cache
pub struct InMemoryTokenStore {
    cache: Arc<Cache<String, OAuth2State>>,
    completed_sessions: Arc<Cache<String, CompletedAuthSession>>,
}

impl InMemoryTokenStore {
    /// Create a new InMemoryTokenStore with the specified TTL
    pub fn new(ttl: Duration) -> Self {
        let cache = Cache::builder()
            .time_to_live(ttl)
            .max_capacity(10_000) // Prevent memory exhaustion from attacks
            .build();

        // Completed sessions have a shorter TTL (5 minutes) since they should be used immediately
        let completed_sessions = Cache::builder()
            .time_to_live(Duration::from_secs(300))
            .max_capacity(10_000)
            .build();

        Self {
            cache: Arc::new(cache),
            completed_sessions: Arc::new(completed_sessions),
        }
    }
}

impl TokenStore for InMemoryTokenStore {
    fn save(&self, state: String, data: OAuth2State) {
        self.cache.insert(state, data);
    }

    fn get(&self, state: &str) -> Option<OAuth2State> {
        self.cache.get(state)
    }

    fn save_completed_session(&self, token: String, session: CompletedAuthSession) {
        self.completed_sessions.insert(token, session);
    }

    fn get_completed_session(&self, token: &str) -> Option<CompletedAuthSession> {
        // Remove and return (one-time use)
        self.completed_sessions.remove(token)
    }
}

/// Generate a cryptographically secure PKCE code verifier
///
/// The verifier is a random string of 43-128 characters using unreserved characters
/// defined in RFC 3986: [A-Z] / [a-z] / [0-9] / "-" / "." / "_" / "~"
///
/// This implementation generates a 64-character verifier (48 random bytes base64url encoded)
pub fn generate_code_verifier() -> String {
    let mut random_bytes = [0u8; 48];
    rand::thread_rng().fill_bytes(&mut random_bytes);
    Base64UrlUnpadded::encode_string(&random_bytes)
}

/// Generate a PKCE code challenge from a code verifier using S256 method
///
/// code_challenge = BASE64URL(SHA256(ASCII(code_verifier)))
pub fn generate_code_challenge(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let hash = hasher.finalize();
    Base64UrlUnpadded::encode_string(&hash)
}

/// Generate a cryptographically secure random state token
///
/// The state token is used to prevent CSRF attacks in the OAuth2 flow
pub fn generate_state_token() -> String {
    let mut random_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut random_bytes);
    Base64UrlUnpadded::encode_string(&random_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_verifier_length() {
        let verifier = generate_code_verifier();
        // 48 bytes base64url encoded = 64 characters
        assert_eq!(verifier.len(), 64);
        // Verify it only contains valid base64url characters
        assert!(verifier
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn test_code_challenge_deterministic() {
        let verifier = "test_verifier_123";
        let challenge1 = generate_code_challenge(verifier);
        let challenge2 = generate_code_challenge(verifier);
        assert_eq!(challenge1, challenge2);
    }

    #[test]
    fn test_code_challenge_unique() {
        let verifier1 = "verifier1";
        let verifier2 = "verifier2";
        let challenge1 = generate_code_challenge(verifier1);
        let challenge2 = generate_code_challenge(verifier2);
        assert_ne!(challenge1, challenge2);
    }

    #[test]
    fn test_state_token_length() {
        let state = generate_state_token();
        // 32 bytes base64url encoded = 43 characters
        assert_eq!(state.len(), 43);
    }

    #[test]
    fn test_state_token_randomness() {
        let state1 = generate_state_token();
        let state2 = generate_state_token();
        assert_ne!(state1, state2);
    }

    #[test]
    fn test_token_store_save_and_get() {
        let store = InMemoryTokenStore::new(Duration::from_secs(60));
        let state = "test_state";
        let data = OAuth2State {
            code_verifier: "test_verifier".to_string(),
            redirect_url: Some("https://example.com".to_string()),
            project_name: None,
            custom_domain_callback_url: None,
        };

        store.save(state.to_string(), data.clone());
        let retrieved = store.get(state);

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.code_verifier, data.code_verifier);
        assert_eq!(retrieved.redirect_url, data.redirect_url);
    }

    #[test]
    fn test_token_store_get_nonexistent() {
        let store = InMemoryTokenStore::new(Duration::from_secs(60));
        let retrieved = store.get("nonexistent");
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_token_store_ttl() {
        let store = InMemoryTokenStore::new(Duration::from_millis(100));
        let state = "test_state";
        let data = OAuth2State {
            code_verifier: "test_verifier".to_string(),
            redirect_url: None,
            project_name: None,
            custom_domain_callback_url: None,
        };

        store.save(state.to_string(), data);

        // Should exist immediately
        assert!(store.get(state).is_some());

        // Wait for TTL to expire
        std::thread::sleep(Duration::from_millis(150));

        // Should be gone after TTL
        assert!(store.get(state).is_none());
    }
}
