//! Re-export the daemon API from the `simply-daemon-api` crate.
//!
//! All trait definitions and types now live in `simply-daemon-api`.
//! This module re-exports everything for backward compatibility.

pub use simply_daemon_api::*;

// Re-export implementation-specific types that consumers in this crate need
// but that don't belong in the API crate.
pub use simply_core::storage::{
    Entity, EntityType,
    Document, DocumentSource, DocumentTab, StoredEditable,
    DocumentStore, Stores, UserStore,
    FsBlobStore, SqliteStore,
};
pub use simply_core::storage::coordinator::StorageCoordinator;
pub use simply_core::storage::traits::StorageTypes;
pub use simply_core::mcp::{ServerStatus, spawn_retry_task, start_auto_connect};

#[cfg(test)]
mod ts_export {
    use ts_rs::TS;

    #[test]
    fn export_all_types() {
        use simply_daemon_api::*;

        SessionId::export_all().expect("SessionId");
        DaemonEvent::export_all().expect("DaemonEvent");
        InboundEvent::export_all().expect("InboundEvent");
        ConversationInfo::export_all().expect("ConversationInfo");
        EmbeddingQueueStatus::export_all().expect("EmbeddingQueueStatus");
        SessionInfo::export_all().expect("SessionInfo");
        CreateSessionOptions::export_all().expect("CreateSessionOptions");
        UserMessage::export_all().expect("UserMessage");
        SeedMessage::export_all().expect("SeedMessage");
        DocumentInfo::export_all().expect("DocumentInfo");
        DocumentDetail::export_all().expect("DocumentDetail");
        TabInfo::export_all().expect("TabInfo");
        CreateDocumentRequest::export_all().expect("CreateDocumentRequest");
        CreateTabRequest::export_all().expect("CreateTabRequest");
        UpdateTabRequest::export_all().expect("UpdateTabRequest");
        McpServerInfo::export_all().expect("McpServerInfo");
        AddMcpServerRequest::export_all().expect("AddMcpServerRequest");
        RegisterEphemeralRequest::export_all().expect("RegisterEphemeralRequest");
        UpdateMcpServerRequest::export_all().expect("UpdateMcpServerRequest");
        OAuthFlowInfo::export_all().expect("OAuthFlowInfo");
        AssetInfo::export_all().expect("AssetInfo");
        VoiceProviderInfo::export_all().expect("VoiceProviderInfo");
        SearchHit::export_all().expect("SearchHit");
        SearchRequest::export_all().expect("SearchRequest");
        ReindexStatus::export_all().expect("ReindexStatus");
    }
}
