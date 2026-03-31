//! User storage types

/// Core user data
#[derive(Debug, Clone)]
pub struct User {
    pub email: String,
}

impl User {
    /// Create a new user with the given email
    pub fn new(email: impl Into<String>) -> Self {
        Self { email: email.into() }
    }
}
