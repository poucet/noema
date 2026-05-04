//! Shared entity access policy for daemon services.
//!
//! Keep ownership/privacy decisions here so entity browsing, search, chat
//! references, and vault sync do not drift apart as new document workflows
//! are added.

use simply_core::storage::ids::UserId;
use simply_core::storage::traits::StoredEntity;
use simply_rpc::RequestContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityAccess {
    Read,
    Write,
}

pub struct AccessPolicy;

impl AccessPolicy {
    pub fn require_user(ctx: &RequestContext) -> anyhow::Result<UserId> {
        ctx.scope
            .user_id
            .as_ref()
            .map(UserId::from_string)
            .ok_or_else(|| anyhow::anyhow!("authentication required"))
    }

    pub fn can(user_id: Option<&UserId>, entity: &StoredEntity, access: EntityAccess) -> bool {
        match access {
            EntityAccess::Read => Self::can_read(user_id, entity),
            EntityAccess::Write => Self::can_write(user_id, entity),
        }
    }

    /// Owner and system-owned entities are readable. Non-private entities are
    /// readable by other callers, including anonymous search contexts.
    pub fn can_read(user_id: Option<&UserId>, entity: &StoredEntity) -> bool {
        match (&entity.user_id, user_id) {
            (None, _) => true,
            (Some(owner), Some(caller)) if owner == caller => true,
            _ => !entity.is_private,
        }
    }

    /// Mutations are reserved for the owner. System-owned entities preserve
    /// the existing daemon behavior: an authenticated user can manage them.
    pub fn can_write(user_id: Option<&UserId>, entity: &StoredEntity) -> bool {
        match (&entity.user_id, user_id) {
            (None, Some(_)) => true,
            (Some(owner), Some(caller)) if owner == caller => true,
            _ => false,
        }
    }

    pub fn ensure(
        user_id: Option<&UserId>,
        entity: &StoredEntity,
        access: EntityAccess,
    ) -> anyhow::Result<()> {
        if Self::can(user_id, entity, access) {
            return Ok(());
        }

        let action = match access {
            EntityAccess::Read => "read",
            EntityAccess::Write => "modify",
        };
        anyhow::bail!("access denied: caller cannot {action} entity {}", entity.id)
    }

    pub fn ensure_read(user_id: Option<&UserId>, entity: &StoredEntity) -> anyhow::Result<()> {
        Self::ensure(user_id, entity, EntityAccess::Read)
    }

    pub fn ensure_write(user_id: Option<&UserId>, entity: &StoredEntity) -> anyhow::Result<()> {
        Self::ensure(user_id, entity, EntityAccess::Write)
    }

    pub fn privacy_label(entity: &StoredEntity) -> &'static str {
        if entity.is_private {
            "private"
        } else {
            "public"
        }
    }
}
