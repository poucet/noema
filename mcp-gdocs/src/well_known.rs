//! OAuth 2.0 Well-Known Configuration for Google OAuth
//!
//! This module provides the OAuth authorization server metadata that Noema uses
//! to initiate the OAuth flow with Google.

use serde::Serialize;

/// OAuth 2.0 Authorization Server Metadata (RFC 8414)
#[derive(Debug, Clone, Serialize)]
pub struct OAuthServerMetadata {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registration_endpoint: Option<String>,
    pub scopes_supported: Vec<String>,
    pub response_types_supported: Vec<String>,
    pub grant_types_supported: Vec<String>,
    pub code_challenge_methods_supported: Vec<String>,
}

/// Get the OAuth server metadata for Google
pub fn google_oauth_metadata() -> OAuthServerMetadata {
    OAuthServerMetadata {
        issuer: "https://accounts.google.com".to_string(),
        authorization_endpoint: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
        token_endpoint: "https://oauth2.googleapis.com/token".to_string(),
        registration_endpoint: None, // Google doesn't support dynamic client registration
        scopes_supported: vec![
            "https://www.googleapis.com/auth/documents.readonly".to_string(),
            "https://www.googleapis.com/auth/drive.readonly".to_string(),
        ],
        response_types_supported: vec!["code".to_string()],
        grant_types_supported: vec![
            "authorization_code".to_string(),
            "refresh_token".to_string(),
        ],
        code_challenge_methods_supported: vec!["S256".to_string()],
    }
}
