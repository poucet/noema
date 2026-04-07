//! Authentication types and request user resolution.

use simply_core::storage::ids::UserId;

/// The resolved identity for a request.
///
/// Determined by the auth middleware:
/// - Localhost → `Admin`
/// - `daemon_secret` bearer → `Service` (trusted client, may assert X-User-Id)
/// - Future: passkey session → `Admin` (non-localhost)
/// - No auth → `Anonymous`
#[derive(Debug, Clone)]
pub enum RequestUser {
    /// Admin — localhost access or authenticated via passkey (future).
    Admin,
    /// Trusted service client (Noema, Lumina) using daemon_secret.
    /// Optional UserId from X-User-Id header.
    Service(Option<UserId>),
    /// No authentication provided.
    Anonymous,
}

impl RequestUser {
    /// The effective user ID, if any.
    pub fn user_id(&self) -> Option<&UserId> {
        match self {
            RequestUser::Service(id) => id.as_ref(),
            _ => None,
        }
    }

    pub fn is_admin(&self) -> bool {
        matches!(self, RequestUser::Admin)
    }

    pub fn is_anonymous(&self) -> bool {
        matches!(self, RequestUser::Anonymous)
    }
}
