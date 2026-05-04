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

#[cfg(test)]
mod tests {
    use super::*;
    use simply_core::storage::ids::EntityId;
    use simply_core::storage::types::{stored_editable, Entity, EntityType};
    use simply_rpc::Scope;

    fn stored_entity(owner: Option<UserId>, is_private: bool) -> StoredEntity {
        let mut entity = Entity::new(EntityType::document_note());
        entity.user_id = owner;
        entity.is_private = is_private;
        stored_editable(EntityId::from_string("entity-1"), entity, 1, 1)
    }

    #[test]
    fn owner_can_read_and_write_private_entity() {
        let owner = UserId::from_string("user-1");
        let entity = stored_entity(Some(owner.clone()), true);

        assert!(AccessPolicy::can_read(Some(&owner), &entity));
        assert!(AccessPolicy::can_write(Some(&owner), &entity));
        AccessPolicy::ensure_read(Some(&owner), &entity).unwrap();
        AccessPolicy::ensure_write(Some(&owner), &entity).unwrap();
    }

    #[test]
    fn public_entity_is_readable_but_not_writable_by_non_owner() {
        let owner = UserId::from_string("user-1");
        let caller = UserId::from_string("user-2");
        let entity = stored_entity(Some(owner), false);

        assert!(AccessPolicy::can_read(Some(&caller), &entity));
        assert!(AccessPolicy::can_read(None, &entity));
        assert!(!AccessPolicy::can_write(Some(&caller), &entity));
        assert!(!AccessPolicy::can_write(None, &entity));
        assert!(AccessPolicy::ensure_write(Some(&caller), &entity).is_err());
    }

    #[test]
    fn private_entity_is_not_visible_to_non_owner() {
        let owner = UserId::from_string("user-1");
        let caller = UserId::from_string("user-2");
        let entity = stored_entity(Some(owner), true);

        assert!(!AccessPolicy::can_read(Some(&caller), &entity));
        assert!(!AccessPolicy::can_read(None, &entity));
        assert!(!AccessPolicy::can_write(Some(&caller), &entity));
        assert!(AccessPolicy::ensure_read(Some(&caller), &entity).is_err());
    }

    #[test]
    fn unowned_entities_are_readable_and_authenticated_writable() {
        let caller = UserId::from_string("user-1");
        let entity = stored_entity(None, true);

        assert!(AccessPolicy::can_read(None, &entity));
        assert!(AccessPolicy::can_read(Some(&caller), &entity));
        assert!(!AccessPolicy::can_write(None, &entity));
        assert!(AccessPolicy::can_write(Some(&caller), &entity));
    }

    #[test]
    fn require_user_extracts_authenticated_scope() {
        let user = UserId::from_string("user-1");
        let ctx = RequestContext::with_scope(Scope::user(user.as_str()));

        assert_eq!(AccessPolicy::require_user(&ctx).unwrap(), user);
        assert!(AccessPolicy::require_user(&RequestContext::anonymous()).is_err());
    }

    #[test]
    fn privacy_label_reflects_entity_visibility() {
        assert_eq!(AccessPolicy::privacy_label(&stored_entity(None, true)), "private");
        assert_eq!(AccessPolicy::privacy_label(&stored_entity(None, false)), "public");
    }
}
