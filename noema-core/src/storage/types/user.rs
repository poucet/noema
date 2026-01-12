//! User storage types

use crate::storage::ids::UserId;

/// Information about a user
#[derive(Debug, Clone)]
pub struct UserInfo {
    pub id: UserId,
    pub email: String,
}
