//! JWT encode/decode for Kainetic Cloud session tokens.
//!
//! Uses HMAC-SHA256 (`HS256`).  Claims are minimal: subject (user UUID),
//! team UUID, RBAC role, and standard `exp`/`iat`.

use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::error::CloudError;

/// RBAC roles that can be embedded in a JWT.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Read-only access.
    Viewer,
    /// Can register agents and submit runs.
    Developer,
    /// Full access including team management and key rotation.
    Admin,
}

/// JWT claims payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject — the authenticated user UUID.
    pub sub: String,
    /// The team this token is scoped to.
    pub team_id: String,
    /// RBAC role within the team.
    pub role: Role,
    /// Standard expiry (Unix timestamp).
    pub exp: u64,
    /// Standard issued-at (Unix timestamp).
    pub iat: u64,
}

/// Encodes a `Claims` struct into a signed JWT string.
///
/// # Errors
///
/// Returns [`CloudError::Internal`] if encoding fails.
pub fn encode_token(claims: &Claims, secret: &str) -> Result<String, CloudError> {
    encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| CloudError::Internal(format!("jwt encode failed: {e}")))
}

/// Decodes and validates a JWT string, returning the embedded [`Claims`].
///
/// # Errors
///
/// Returns [`CloudError::Unauthorized`] if the token is expired, malformed, or
/// the signature does not verify.
pub fn decode_token(token: &str, secret: &str) -> Result<Claims, CloudError> {
    let mut validation = Validation::default();
    validation.validate_exp = true;

    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|d| d.claims)
    .map_err(|e| CloudError::Unauthorized(format!("invalid token: {e}")))
}

/// Builds a [`Claims`] with the given fields and an expiry derived from `ttl_secs`.
#[must_use]
pub fn build_claims(user_id: &str, team_id: &str, role: Role, ttl_secs: u64) -> Claims {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    Claims {
        sub: user_id.to_string(),
        team_id: team_id.to_string(),
        role,
        exp: now + ttl_secs,
        iat: now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-secret-32-bytes-exactly!!xx";

    fn test_claims() -> Claims {
        build_claims("user-uuid-1234", "team-uuid-5678", Role::Developer, 3600)
    }

    #[test]
    fn roundtrip_encode_decode() {
        let claims = test_claims();
        let token = encode_token(&claims, SECRET).unwrap();
        let decoded = decode_token(&token, SECRET).unwrap();
        assert_eq!(decoded.sub, "user-uuid-1234");
        assert_eq!(decoded.team_id, "team-uuid-5678");
        assert_eq!(decoded.role, Role::Developer);
    }

    #[test]
    fn wrong_secret_returns_unauthorized() {
        let token = encode_token(&test_claims(), SECRET).unwrap();
        let err = decode_token(&token, "different-secret").unwrap_err();
        assert!(matches!(err, CloudError::Unauthorized(_)));
    }

    #[test]
    fn expired_token_returns_unauthorized() {
        let mut claims = test_claims();
        claims.exp = 1; // epoch+1 — always expired
        let token = encode_token(&claims, SECRET).unwrap();
        let err = decode_token(&token, SECRET).unwrap_err();
        assert!(matches!(err, CloudError::Unauthorized(_)));
    }

    #[test]
    fn role_serde_roundtrip() {
        let json = serde_json::to_string(&Role::Admin).unwrap();
        assert_eq!(json, r#""admin""#);
        let back: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Role::Admin);
    }
}
