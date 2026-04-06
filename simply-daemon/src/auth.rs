//! Authentication types and request user resolution.

use simply_core::storage::ids::UserId;

/// User access tier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserTier {
    /// Full access — Google account matches admin_email in settings
    Admin,
    /// Own data — authenticated via OAuth, has a UCM user record
    Authenticated,
    /// Public read-only — no authentication
    Anonymous,
}

/// The resolved identity for a request.
///
/// Determined by the auth middleware from the Bearer token:
/// - `daemon_secret` → `Service` (trusted client, may assert X-User-Id)
/// - `user_token` → `User` (identity bound to token, not spoofable)
/// - no token → `Anonymous`
#[derive(Debug, Clone)]
pub enum RequestUser {
    /// Trusted service client (Noema, Lumina) using daemon_secret.
    /// The optional UserId comes from X-User-Id header — trusted because
    /// only service clients can present the daemon_secret.
    Service(Option<UserId>),
    /// Authenticated user via per-user token (OAuth-issued).
    User(UserId),
    /// No authentication provided.
    Anonymous,
}

impl RequestUser {
    /// The effective user ID, if any.
    pub fn user_id(&self) -> Option<&UserId> {
        match self {
            RequestUser::Service(id) => id.as_ref(),
            RequestUser::User(id) => Some(id),
            RequestUser::Anonymous => None,
        }
    }

    pub fn is_service(&self) -> bool {
        matches!(self, RequestUser::Service(_))
    }

    pub fn is_anonymous(&self) -> bool {
        matches!(self, RequestUser::Anonymous)
    }
}
