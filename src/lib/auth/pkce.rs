//! PKCE (Proof Key for Code Exchange) implementation.
//!
//! Generates cryptographically secure code verifiers and their SHA256 challenges
//! as required by the Spotify Authorization Code with PKCE flow.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;
use sha2::{Digest, Sha256};

/// Characters allowed in the PKCE code verifier (RFC 7636).
const VERIFIER_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

/// PKCE challenge containing both the verifier and its SHA256 challenge.
#[derive(Debug, Clone)]
pub struct PkceChallenge {
    /// The original random code verifier (43-128 characters).
    pub verifier: String,
    /// The base64url-encoded SHA256 hash of the verifier.
    pub challenge: String,
}

impl PkceChallenge {
    /// Generates a new PKCE challenge with a 64-character verifier.
    pub fn generate() -> Self {
        Self::generate_with_length(64)
    }

    /// Generates a new PKCE challenge with a custom verifier length.
    ///
    /// # Panics
    /// Panics if length is not between 43 and 128 (inclusive).
    pub fn generate_with_length(length: usize) -> Self {
        assert!(
            (43..=128).contains(&length),
            "PKCE verifier length must be between 43 and 128 characters"
        );

        let verifier = generate_random_string(length);
        let challenge = generate_challenge(&verifier);

        Self { verifier, challenge }
    }
}

/// Generates a cryptographically random string of the specified length
/// using characters valid for PKCE code verifiers.
pub fn generate_random_string(length: usize) -> String {
    let mut rng = rand::rng();
    (0..length)
        .map(|_| {
            let idx = rng.random_range(0..VERIFIER_CHARS.len());
            VERIFIER_CHARS[idx] as char
        })
        .collect()
}

/// Generates the code challenge from a code verifier.
/// Uses SHA256 hash encoded as base64url without padding.
fn generate_challenge(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let hash = hasher.finalize();
    URL_SAFE_NO_PAD.encode(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verifier_length() {
        let pkce = PkceChallenge::generate();
        assert_eq!(pkce.verifier.len(), 64);
    }

    #[test]
    fn test_verifier_custom_length() {
        let pkce = PkceChallenge::generate_with_length(128);
        assert_eq!(pkce.verifier.len(), 128);
    }

    #[test]
    fn test_verifier_characters() {
        let pkce = PkceChallenge::generate();
        for c in pkce.verifier.chars() {
            assert!(VERIFIER_CHARS.contains(&(c as u8)));
        }
    }

    #[test]
    fn test_challenge_is_base64url() {
        let pkce = PkceChallenge::generate();
        // SHA256 produces 32 bytes, base64url encoded without padding = 43 characters
        assert_eq!(pkce.challenge.len(), 43);
        // Verify it can be decoded
        assert!(URL_SAFE_NO_PAD.decode(&pkce.challenge).is_ok());
    }

    #[test]
    #[should_panic(expected = "PKCE verifier length must be between 43 and 128")]
    fn test_verifier_too_short() {
        PkceChallenge::generate_with_length(42);
    }

    #[test]
    #[should_panic(expected = "PKCE verifier length must be between 43 and 128")]
    fn test_verifier_too_long() {
        PkceChallenge::generate_with_length(129);
    }
}
