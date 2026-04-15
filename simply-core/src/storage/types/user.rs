//! User storage types

/// Core user data
#[derive(Debug, Clone)]
pub struct User {
    /// Email address (optional — external-identity users may not have one)
    pub email: Option<String>,
}

impl User {
    /// Create a new user with the given email
    pub fn new(email: impl Into<String>) -> Self {
        Self { email: Some(email.into()) }
    }

    /// Create a user with no email (external identity only)
    pub fn anonymous() -> Self {
        Self { email: None }
    }
}
