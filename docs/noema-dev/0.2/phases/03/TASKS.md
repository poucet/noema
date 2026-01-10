# Phase 3: Unified Content Model

## Overview

Phase 3 establishes the **Unified Content Model** - separating immutable content from mutable structure. This enables parallel model responses, conversation forking, document versioning, and flexible organization.

**Core Principle**: Content (text, assets) is heavy and immutable. Structure (conversations, documents, collections) is lightweight and mutable.

## Task Table

| Status | Pri | # | Feature | Description |
|--------|-----|---|---------|-------------|
| ⬜ | P0 | 3.1 | Content blocks | Content-addressed text storage with origin tracking |
| ⬜ | P0 | 3.1b | Asset storage | Binary blob storage (images, audio, PDFs) |
| ⬜ | P0 | 3.2 | Conversation structure | Turns, spans, messages with content references |
| ⬜ | P0 | 3.3 | Views and forking | Named paths through conversations, fork support |
| ⬜ | P1 | 3.4 | Document structure | Documents with tabs and revision history |
| ⬜ | P1 | 3.5 | Collections | Tree organization with tags and fields |
| ⬜ | P1 | 3.6 | Cross-references | Links between any entities with backlinks |
| ⬜ | P2 | 3.7 | Temporal queries | Time-based activity summaries for LLM context |
| ⬜ | P2 | 3.8 | Session integration | Connect engine to new conversation model |
| ⬜ | P2 | 3.9 | Migration and cleanup | Remove legacy tables |

Status: ⬜ todo, 🔄 in-progress, ✅ done, 🚫 blocked, ⏸️ deferred

---

## Microtasks (Commit-Sized Steps)

Each microtask is a single atomic commit. Complete in order within each feature.

---

### 3.1 Content Blocks

#### 3.1.1 Define type-safe IDs module
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add storage/ids.rs with typed ID newtypes` |
| **Create** | `noema-core/src/storage/ids.rs` |
| **Implement** | `define_id!` macro, `ContentBlockId`, `AssetId`, `ConversationId`, `TurnId`, `SpanId`, `MessageId`, `ViewId`, `DocumentId`, `TabId`, `RevisionId`, `CollectionId`, `CollectionItemId`, `ReferenceId`, `UserId` |
| **Update** | `noema-core/src/storage/mod.rs` - add `pub mod ids` |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.1.2 Create ContentOrigin and OriginKind types
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add content origin types for provenance tracking` |
| **Create** | `noema-core/src/storage/content_block/types.rs` |
| **Implement** | `OriginKind` enum (User, Assistant, System, Import), `ContentOrigin` struct with `kind`, `user_id`, `model_id`, `source_id`, `parent_id`, `is_private` |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.1.3 Define ContentBlockStore trait
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add ContentBlockStore trait with async methods` |
| **Create** | `noema-core/src/storage/content_block/mod.rs` |
| **Implement** | `ContentBlockStore` trait with `store()`, `get()`, `get_text()`, `exists()`, `find_by_hash()` |
| **Implement** | `ContentBlockInfo` struct for query results |
| **Update** | `noema-core/src/storage/mod.rs` - add `pub mod content_block` |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.1.4 Add content_blocks table migration
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add content_blocks schema with hash, origin, privacy` |
| **Update** | `noema-core/src/storage/sqlite/schema.rs` or migrations |
| **SQL** | `CREATE TABLE content_blocks (id, content_hash, content_type, text, is_private, origin_kind, origin_user_id, origin_model_id, origin_source_id, origin_parent_id, created_at)` |
| **SQL** | `CREATE INDEX idx_content_blocks_hash`, `idx_content_blocks_origin`, `idx_content_blocks_private` |
| **Verify** | Fresh DB creates table, `cargo build` compiles |

#### 3.1.5 Implement SqliteContentBlockStore
| Status | ⬜ |
|--------|-----|
| **Commit** | `Implement ContentBlockStore for SQLite` |
| **Create** | `noema-core/src/storage/content_block/sqlite.rs` |
| **Implement** | `SqliteContentBlockStore` with all trait methods |
| **Implement** | SHA-256 hashing on store, dedup check via hash lookup |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.1.6 Add content_block_tags table
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add tag support for content scoping` |
| **Update** | Schema with `CREATE TABLE content_block_tags (content_id, tag, PRIMARY KEY)` |
| **Implement** | `tag()`, `untag()`, `get_tags()`, `find_by_tag()` methods |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.1.7 Unit tests for content block store
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add tests for content block CRUD and dedup` |
| **Create** | `noema-core/src/storage/content_block/tests.rs` |
| **Test** | Store text → get UUID → retrieve text → verify match |
| **Test** | Store same text twice → same hash returned |
| **Test** | Store with origin → retrieve → origin preserved |
| **Test** | Tag content → find by tag → verify found |
| **Verify** | `cargo test -p noema-core content_block` passes |

---

### 3.1b Asset Storage

#### 3.1b.1 Define AssetStore trait
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add AssetStore trait for binary blobs` |
| **Create** | `noema-core/src/storage/asset/mod.rs` |
| **Implement** | `AssetStore` trait with `store()`, `get()`, `get_data()`, `exists()` |
| **Implement** | `AssetInfo` struct with `id`, `mime_type`, `filename`, `size_bytes`, `is_private` |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.1b.2 Add assets table migration
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add assets schema with hash, mime, privacy` |
| **SQL** | `CREATE TABLE assets (id, mime_type, original_filename, file_size_bytes, is_private, metadata_json, local_path, created_at)` |
| **Verify** | Fresh DB creates table |

#### 3.1b.3 Implement SqliteAssetStore
| Status | ⬜ |
|--------|-----|
| **Commit** | `Implement AssetStore for SQLite with blob storage` |
| **Create** | `noema-core/src/storage/asset/sqlite.rs` |
| **Implement** | SHA-256 hash as ID, store bytes to `blob_storage/{hash[0:2]}/{hash}` |
| **Implement** | Dedup: if hash exists, return existing ID |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.1b.4 Add AssetRef to StoredContent enum
| Status | ⬜ |
|--------|-----|
| **Commit** | `Extend StoredContent with AssetRef variant` |
| **Update** | `noema-core/src/storage/payload.rs` (or equivalent) |
| **Add** | `AssetRef { asset_id: String, mime_type: String, filename: Option<String> }` variant |
| **Verify** | `cargo build -p noema-core` compiles, existing code still works |

#### 3.1b.5 Implement asset resolution in payload
| Status | ⬜ |
|--------|-----|
| **Commit** | `Resolve AssetRef to inline base64 for LLM` |
| **Update** | `StoredPayload::resolve()` or equivalent |
| **Implement** | When resolving, fetch asset data via `AssetStore::get_data()`, convert to base64, return as `Image` or `Audio` |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.1b.6 Unit tests for asset store
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add tests for asset storage and resolution` |
| **Test** | Store image bytes → get hash ID → retrieve bytes → verify match |
| **Test** | Store same bytes twice → same hash returned (dedup) |
| **Test** | Create message with AssetRef → resolve → verify inline base64 |
| **Test** | Private asset + cloud model context → verify excluded |
| **Verify** | `cargo test -p noema-core asset` passes |

---

### 3.2 Conversation Structure

#### 3.2.1 Define Turn, Span, Message types
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add conversation structure types` |
| **Create** | `noema-core/src/storage/conversation/types.rs` |
| **Implement** | `TurnInfo`, `SpanInfo`, `MessageInfo` structs |
| **Implement** | `SpanRole` enum (User, Assistant) |
| **Implement** | `NewMessage` input struct |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.2.2 Add conversations table migration
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add conversations schema with parent_span_id` |
| **SQL** | `CREATE TABLE conversations (id, user_id, title, system_prompt, is_private, parent_span_id, created_at, updated_at)` |
| **SQL** | `CREATE INDEX idx_conversations_parent` |
| **Verify** | Fresh DB creates table |

#### 3.2.3 Add turns table migration
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add turns schema with sequence ordering` |
| **SQL** | `CREATE TABLE turns (id, conversation_id, sequence_number, created_at)` |
| **SQL** | `UNIQUE (conversation_id, sequence_number)`, `CREATE INDEX idx_turns_conversation` |
| **Verify** | Fresh DB creates table |

#### 3.2.4 Add spans table migration
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add spans schema with role and model_id` |
| **SQL** | `CREATE TABLE spans (id, turn_id, role, model_id, parent_span_id, created_at)` |
| **SQL** | `CHECK(role IN ('user', 'assistant'))`, indexes |
| **Verify** | Fresh DB creates table |

#### 3.2.5 Add messages table migration
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add messages schema referencing content_blocks` |
| **SQL** | `CREATE TABLE messages (id, span_id, sequence_number, role, content_id, tool_calls, tool_results, created_at)` |
| **SQL** | `FOREIGN KEY (content_id) REFERENCES content_blocks(id)` |
| **Verify** | Fresh DB creates table with FK constraint |

#### 3.2.6 Define ConversationStore trait
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add ConversationStore trait for turn/span/message ops` |
| **Create** | `noema-core/src/storage/conversation/mod.rs` |
| **Implement** | Trait with turn, span, message methods (signatures only) |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.2.7 Implement add_turn and get_turns
| Status | ⬜ |
|--------|-----|
| **Commit** | `Implement turn management in SqliteConversationStore` |
| **Create** | `noema-core/src/storage/conversation/sqlite.rs` |
| **Implement** | `add_turn()` - auto-increment sequence_number |
| **Implement** | `get_turns()` - ordered by sequence_number |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.2.8 Implement add_span and get_spans
| Status | ⬜ |
|--------|-----|
| **Commit** | `Implement span management with role tracking` |
| **Implement** | `add_span(turn_id, role, model_id)` |
| **Implement** | `add_child_span(parent_span_id, role, model_id)` for sub-conversations |
| **Implement** | `get_spans(turn_id)`, `get_child_spans(span_id)` |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.2.9 Implement add_message and get_messages
| Status | ⬜ |
|--------|-----|
| **Commit** | `Implement message management with content refs` |
| **Implement** | `add_message(span_id, NewMessage)` - stores text via ContentBlockStore, saves content_id |
| **Implement** | `get_messages(span_id)` - ordered by sequence_number, joins content_blocks for text |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.2.10 Unit tests for conversation structure
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add tests for turn/span/message CRUD` |
| **Test** | Create conversation → add turn → add span → add message → verify chain |
| **Test** | Add multiple spans to same turn → verify both exist |
| **Test** | Span with multiple messages (tool flow) → verify order preserved |
| **Test** | Message text stored in content_blocks → verify searchable |
| **Verify** | `cargo test -p noema-core conversation` passes |

---

### 3.3 Views and Forking

#### 3.3.1 Add views table migration
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add views schema with fork tracking` |
| **SQL** | `CREATE TABLE views (id, conversation_id, name, is_main, forked_from_view_id, forked_at_turn_id, created_at)` |
| **Verify** | Fresh DB creates table |

#### 3.3.2 Add view_selections table migration
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add view_selections for span choices per turn` |
| **SQL** | `CREATE TABLE view_selections (view_id, turn_id, span_id, PRIMARY KEY (view_id, turn_id))` |
| **Verify** | Fresh DB creates table |

#### 3.3.3 Implement create_view
| Status | ⬜ |
|--------|-----|
| **Commit** | `Implement view creation with main flag` |
| **Implement** | `create_view(conversation_id, name, is_main)` |
| **Implement** | Auto-create main view when conversation created |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.3.4 Implement select_span
| Status | ⬜ |
|--------|-----|
| **Commit** | `Implement span selection for views` |
| **Implement** | `select_span(view_id, turn_id, span_id)` - upsert into view_selections |
| **Implement** | Auto-select first span when turn added (for main view) |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.3.5 Implement get_view_path
| Status | ⬜ |
|--------|-----|
| **Commit** | `Implement path traversal through selected spans` |
| **Implement** | `get_view_path(view_id)` → `Vec<(Turn, Span, Vec<Message>)>` |
| **Implement** | Join turns → view_selections → spans → messages, ordered |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.3.6 Implement fork_view
| Status | ⬜ |
|--------|-----|
| **Commit** | `Implement view forking with shared prefix` |
| **Implement** | `fork_view(view_id, at_turn_id)` |
| **Implement** | Copy selections for turns before fork point |
| **Implement** | Set `forked_from_view_id` and `forked_at_turn_id` |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.3.7 Implement edit_turn (splice)
| Status | ⬜ |
|--------|-----|
| **Commit** | `Implement turn editing with new span creation` |
| **Implement** | `edit_turn(view_id, turn_id, new_content)` → creates new span at turn |
| **Implement** | `fork_view_with_selections(view_id, selections)` → custom span choices |
| **Implement** | `get_view_context_at(view_id, turn_id)` → messages up to turn |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.3.8 Unit tests for views and forking
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add tests for view CRUD and fork operations` |
| **Test** | Create view → select spans → get path → verify correct messages |
| **Test** | Fork at turn 3 → verify turns 1-2 shared, turn 3+ independent |
| **Test** | Edit turn 2 → create new span → fork with splice → verify mixed old/new |
| **Test** | Two views select different spans at same turn → verify both paths work |
| **Verify** | `cargo test -p noema-core view` passes |

---

### 3.4 Document Structure

#### 3.4.1 Define Document, Tab, Revision types
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add document structure types` |
| **Create** | `noema-core/src/storage/document/types.rs` |
| **Implement** | `DocumentInfo`, `TabInfo`, `RevisionInfo` structs |
| **Implement** | `DocumentSource` enum (UserCreated, AiGenerated, GoogleDrive, Import, Promoted) |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.4.2 Add documents table migration
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add documents schema with source tracking` |
| **SQL** | `CREATE TABLE documents (id, user_id, title, source, source_id, created_at, updated_at)` |
| **Verify** | Fresh DB creates table |

#### 3.4.3 Add document_tabs table migration
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add tabs schema with hierarchy and position` |
| **SQL** | `CREATE TABLE document_tabs (id, document_id, parent_tab_id, title, icon, position, current_revision_id, created_at, updated_at)` |
| **Verify** | Fresh DB creates table |

#### 3.4.4 Add revisions table migration
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add revisions schema referencing content_blocks` |
| **SQL** | `CREATE TABLE revisions (id, tab_id, content_id, parent_revision_id, revision_number, created_at)` |
| **SQL** | `FOREIGN KEY (content_id) REFERENCES content_blocks(id)` |
| **Verify** | Fresh DB creates table |

#### 3.4.5 Define DocumentStore trait
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add DocumentStore trait for doc/tab/revision ops` |
| **Create** | `noema-core/src/storage/document/mod.rs` |
| **Implement** | Trait with document, tab, revision methods |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.4.6 Implement document CRUD
| Status | ⬜ |
|--------|-----|
| **Commit** | `Implement document creation and retrieval` |
| **Create** | `noema-core/src/storage/document/sqlite.rs` |
| **Implement** | `create()`, `get()`, `find_by_source()`, `list()`, `delete()` |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.4.7 Implement tab management
| Status | ⬜ |
|--------|-----|
| **Commit** | `Implement tab add/move/get with hierarchy` |
| **Implement** | `add_tab(document_id, parent_tab_id, title, content)` |
| **Implement** | `add_tab_from_content(document_id, parent_tab_id, title, content_id)` |
| **Implement** | `get_tabs(document_id)` - returns tree structure |
| **Implement** | `move_tab(tab_id, new_parent, position)` |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.4.8 Implement revision commit/checkout
| Status | ⬜ |
|--------|-----|
| **Commit** | `Implement revision chain with branching` |
| **Implement** | `commit(tab_id, content)` - creates revision, updates current_revision_id |
| **Implement** | `branch(tab_id, from_revision_id, content)` - creates revision with different parent |
| **Implement** | `checkout(tab_id, revision_id)` - moves current pointer |
| **Implement** | `get_revisions(tab_id)`, `get_content(revision_id)` |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.4.9 Implement promote_from_message
| Status | ⬜ |
|--------|-----|
| **Commit** | `Implement message-to-document promotion` |
| **Implement** | `promote_from_message(user_id, title, message_id, content_id)` |
| **Implement** | Creates document with `source: Promoted { message_id }` |
| **Implement** | First revision reuses existing content_id (no copy) |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.4.10 Unit tests for document structure
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add tests for doc/tab/revision CRUD` |
| **Test** | Create document → add tab → verify hierarchy |
| **Test** | Add sub-tabs → verify tree structure |
| **Test** | Commit multiple revisions → verify chain |
| **Test** | Branch from rev 2 → checkout branch → verify content |
| **Test** | Promote message → verify content_id reused |
| **Verify** | `cargo test -p noema-core document` passes |

---

### 3.5 Collections

#### 3.5.1 Define Collection, Item, View types
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add collection structure types` |
| **Create** | `noema-core/src/storage/collection/types.rs` |
| **Implement** | `CollectionInfo`, `ItemInfo`, `CollectionViewInfo` structs |
| **Implement** | `ItemTarget` enum (Document, Conversation, ContentBlock, Collection) |
| **Implement** | `FieldDefinition`, `ViewConfig`, `ViewType` types |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.5.2 Add collections table migration
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add collections schema with schema_hint` |
| **SQL** | `CREATE TABLE collections (id, user_id, name, description, schema_hint, created_at, updated_at)` |
| **Verify** | Fresh DB creates table |

#### 3.5.3 Add collection_items table migration
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add items schema with target polymorphism` |
| **SQL** | `CREATE TABLE collection_items (id, collection_id, target_type, target_id, parent_item_id, position, created_at)` |
| **SQL** | Indexes on collection_id, target, parent |
| **Verify** | Fresh DB creates table |

#### 3.5.4 Add item_fields table migration
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add fields as cached index from frontmatter` |
| **SQL** | `CREATE TABLE item_fields (item_id, field_name, field_value, PRIMARY KEY)` |
| **SQL** | `CREATE INDEX idx_item_fields_field` |
| **Verify** | Fresh DB creates table |

#### 3.5.5 Add item_tags table migration
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add tags for cross-cutting organization` |
| **SQL** | `CREATE TABLE item_tags (item_id, tag, PRIMARY KEY)` |
| **SQL** | `CREATE INDEX idx_item_tags_tag` |
| **Verify** | Fresh DB creates table |

#### 3.5.6 Add collection_views table migration
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add saved views with sort/filter config` |
| **SQL** | `CREATE TABLE collection_views (id, collection_id, name, view_type, config, is_default, created_at)` |
| **Verify** | Fresh DB creates table |

#### 3.5.7 Define CollectionStore trait
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add CollectionStore trait` |
| **Create** | `noema-core/src/storage/collection/mod.rs` |
| **Implement** | Trait with collection, item, tag, field, view methods |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.5.8 Implement collection CRUD
| Status | ⬜ |
|--------|-----|
| **Commit** | `Implement collection creation and schema` |
| **Create** | `noema-core/src/storage/collection/sqlite.rs` |
| **Implement** | `create()`, `get()`, `update_schema_hint()`, `delete()` |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.5.9 Implement item management
| Status | ⬜ |
|--------|-----|
| **Commit** | `Implement item add/move/remove with tree` |
| **Implement** | `add_item(collection_id, target, parent_id, position)` |
| **Implement** | `move_item(item_id, new_parent_id, new_position)` - reorder tree |
| **Implement** | `remove_item(item_id)` - cascade to children |
| **Implement** | `get_items(collection_id)` - returns tree |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.5.10 Implement field and tag operations
| Status | ⬜ |
|--------|-----|
| **Commit** | `Implement field caching and tag management` |
| **Implement** | `update_item_fields(item_id, fields)` - upsert to item_fields |
| **Implement** | `reindex_item_fields(item_id)` - parse frontmatter, update cache |
| **Implement** | `tag(item_id, tags)`, `untag(item_id, tags)` |
| **Implement** | `find_by_tag(collection_id, tag)` |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.5.11 Implement view creation and query
| Status | ⬜ |
|--------|-----|
| **Commit** | `Implement saved views with filter/sort` |
| **Implement** | `create_view(collection_id, name, view_type, config)` |
| **Implement** | `query_view(view_id)` - apply sort/filter from config |
| **Implement** | Filter by field value, sort by field |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.5.12 Unit tests for collections
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add tests for collection CRUD and queries` |
| **Test** | Create collection with schema_hint → add items → verify structure |
| **Test** | Add nested items → move item → verify tree updated |
| **Test** | Tag items → find by tag → verify results |
| **Test** | Set fields → create view with filter → query → verify filtered |
| **Test** | Create table view with sort → verify order |
| **Verify** | `cargo test -p noema-core collection` passes |

---

### 3.6 Cross-References

#### 3.6.1 Define Reference and EntityRef types
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add reference types for any-to-any links` |
| **Create** | `noema-core/src/storage/reference/types.rs` |
| **Implement** | `ReferenceInfo` struct, `EntityRef { entity_type, entity_id }` |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.6.2 Add references table migration
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add references schema with relation types` |
| **SQL** | `CREATE TABLE references (id, from_type, from_id, to_type, to_id, relation_type, created_at)` |
| **SQL** | `UNIQUE (from_type, from_id, to_type, to_id, relation_type)` |
| **SQL** | Indexes on from and to |
| **Verify** | Fresh DB creates table |

#### 3.6.3 Define ReferenceStore trait
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add ReferenceStore trait` |
| **Create** | `noema-core/src/storage/reference/mod.rs` |
| **Implement** | Trait with `create()`, `delete()`, `get_outgoing()`, `get_backlinks()` |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.6.4 Implement create and delete
| Status | ⬜ |
|--------|-----|
| **Commit** | `Implement reference creation and deletion` |
| **Create** | `noema-core/src/storage/reference/sqlite.rs` |
| **Implement** | `create(from, to, relation)` - insert, handle unique conflict |
| **Implement** | `delete(id)` |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.6.5 Implement get_outgoing
| Status | ⬜ |
|--------|-----|
| **Commit** | `Implement outgoing reference queries` |
| **Implement** | `get_outgoing(from: EntityRef)` → `Vec<ReferenceInfo>` |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.6.6 Implement get_backlinks
| Status | ⬜ |
|--------|-----|
| **Commit** | `Implement backlink queries` |
| **Implement** | `get_backlinks(to: EntityRef)` → `Vec<ReferenceInfo>` |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.6.7 Unit tests for references
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add tests for reference CRUD and backlinks` |
| **Test** | Create reference → get outgoing → verify found |
| **Test** | Create reference → get backlinks on target → verify found |
| **Test** | Delete reference → verify not found |
| **Test** | Multiple references to same target → verify all in backlinks |
| **Verify** | `cargo test -p noema-core reference` passes |

---

### 3.7 Temporal Queries

#### 3.7.1 Add temporal indexes to tables
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add created_at indexes for time queries` |
| **SQL** | `CREATE INDEX idx_content_blocks_created ON content_blocks(created_at)` |
| **SQL** | `CREATE INDEX idx_messages_created ON messages(created_at)` |
| **SQL** | `CREATE INDEX idx_revisions_created ON revisions(created_at)` |
| **SQL** | `CREATE INDEX idx_conversations_updated ON conversations(updated_at)` |
| **Verify** | Fresh DB creates indexes |

#### 3.7.2 Define TemporalStore trait
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add TemporalStore trait for time-based queries` |
| **Create** | `noema-core/src/storage/temporal/mod.rs` |
| **Implement** | `TemporalStore` trait with `query_by_time_range()`, `get_activity_summary()`, `get_timeline()` |
| **Implement** | `TemporalContent`, `ActivitySummary`, `ContentType` types |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.7.3 Implement query_by_time_range
| Status | ⬜ |
|--------|-----|
| **Commit** | `Implement time range queries across entities` |
| **Create** | `noema-core/src/storage/temporal/sqlite.rs` |
| **Implement** | Query messages, revisions, collection items in range |
| **Implement** | Filter by content_type, limit results |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.7.4 Implement get_activity_summary
| Status | ⬜ |
|--------|-----|
| **Commit** | `Implement activity summarization` |
| **Implement** | Count messages, documents, revisions in range |
| **Implement** | List active conversations |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.7.5 Implement LLM context rendering
| Status | ⬜ |
|--------|-----|
| **Commit** | `Render temporal content as markdown for LLM` |
| **Implement** | `render_activity_context(summary, detail_level)` → markdown string |
| **Implement** | Format with headers, timestamps, previews |
| **Implement** | Respect token budget (truncate if needed) |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.7.6 Unit tests for temporal queries
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add tests for time-based queries` |
| **Test** | Create content at different times → query range → verify correct subset |
| **Test** | Get activity summary → verify counts correct |
| **Test** | Render context → verify markdown format |
| **Verify** | `cargo test -p noema-core temporal` passes |

---

### 3.8 Session Integration

#### 3.8.1 Create adapter types for session
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add adapter bridging old Session to new stores` |
| **Create** | `noema-core/src/storage/session/adapter.rs` |
| **Implement** | `SessionAdapter` holding references to new stores |
| **Implement** | Map old Session methods to new store calls |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.8.2 Implement commit() with new model
| Status | ⬜ |
|--------|-----|
| **Commit** | `Route commit to turn/span/message creation` |
| **Implement** | `commit(user_msg, assistant_msg)` → create turn, span, messages |
| **Implement** | Store text via ContentBlockStore |
| **Implement** | Auto-select span in main view |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.8.3 Implement open_conversation()
| Status | ⬜ |
|--------|-----|
| **Commit** | `Load conversation via view path` |
| **Implement** | `open_conversation(id)` → get main view path |
| **Implement** | Return messages in format engine expects |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.8.4 Implement commit_parallel_responses()
| Status | ⬜ |
|--------|-----|
| **Commit** | `Create multiple spans at same turn` |
| **Implement** | `commit_parallel_responses(user_msg, responses: Vec<Response>)` |
| **Implement** | One turn, multiple spans with different model_ids |
| **Verify** | `cargo build -p noema-core` compiles |

#### 3.8.5 Update engine to use adapter
| Status | ⬜ |
|--------|-----|
| **Commit** | `Wire engine to new session adapter` |
| **Update** | Engine initialization to use SessionAdapter |
| **Update** | Any direct session calls to go through adapter |
| **Verify** | `cargo build -p noema-core` compiles, app starts |

#### 3.8.6 Integration tests with engine
| Status | ⬜ |
|--------|-----|
| **Commit** | `Add tests for full session flow` |
| **Test** | Send message via engine → verify stored in new tables |
| **Test** | Load conversation → verify messages returned correctly |
| **Test** | Parallel responses → verify multiple spans |
| **Verify** | `cargo test -p noema-core session` passes |

---

### 3.9 Migration and Cleanup

#### 3.9.1 Verify all features work with new model
| Status | ⬜ |
|--------|-----|
| **Commit** | `Run full test suite on new schema` |
| **Run** | `cargo test --all` - all tests pass |
| **Run** | Manual app testing - conversations work |
| **Run** | Manual app testing - documents work |
| **Verify** | No regressions |

#### 3.9.2 Drop legacy conversation tables
| Status | ⬜ |
|--------|-----|
| **Commit** | `Remove threads, span_sets, spans, span_messages` |
| **SQL** | `DROP TABLE IF EXISTS span_messages` |
| **SQL** | `DROP TABLE IF EXISTS spans` (old) |
| **SQL** | `DROP TABLE IF EXISTS span_sets` |
| **SQL** | `DROP TABLE IF EXISTS threads` |
| **Verify** | Fresh DB doesn't create old tables |

#### 3.9.3 Drop legacy document tables
| Status | ⬜ |
|--------|-----|
| **Commit** | `Remove old document_tabs, document_revisions` |
| **SQL** | Drop any legacy document tables not matching new schema |
| **Verify** | Fresh DB only has new schema |

#### 3.9.4 Remove old code paths
| Status | ⬜ |
|--------|-----|
| **Commit** | `Clean up deprecated code references` |
| **Remove** | Old session implementation files |
| **Remove** | Old conversation store files |
| **Remove** | References to old table names |
| **Verify** | `cargo build --all` compiles, no dead code warnings |

#### 3.9.5 Final verification
| Status | ⬜ |
|--------|-----|
| **Commit** | `Verify clean database and full functionality` |
| **Test** | Fresh install creates only new tables |
| **Test** | All features work end-to-end |
| **Test** | No references to legacy schema in codebase |
| **Verify** | Phase 3 complete

---

## Feature Details

### Feature 3.1: Content Block Storage

**Problem**: Text content duplicated across messages, documents, revisions. No unified search or cross-referencing.

**Solution**: Content-addressed storage where all text lives in a single table, referenced by ID.

**Functional Requirements**:
- Store text content with type (plain, markdown, typst) and origin metadata
- Track who created content (user, assistant, system, import)
- Track provenance (which model, derived from which parent)
- Same text produces same hash (deduplication)
- Privacy flag marks content as local-only (never sent to cloud models)

**Acceptance Criteria**:
- [ ] Store text → get UUID back
- [ ] Retrieve text by ID
- [ ] Same text → same hash (deduplicated)
- [ ] Origin metadata preserved (user/assistant, model ID, parent ID)
- [ ] Full-text search across all content blocks

---

### Feature 3.1b: Asset Storage

**Problem**: Binary content (images, audio, PDFs) needs separate handling from text.

**Solution**: Content-addressed blob storage with inline references from content.

**Functional Requirements**:
- Store binary blobs by SHA-256 hash (deduplication)
- Track mime type, filename, size
- Privacy flag for local-only assets
- Assets referenced inline from messages/documents as `AssetRef { asset_id, mime_type }`
- Resolve asset references to inline data when sending to LLM

**Acceptance Criteria**:
- [ ] Store image → get hash ID back
- [ ] Same file → same hash (deduplicated)
- [ ] Create message with `AssetRef` pointing to asset
- [ ] Resolve payload converts `AssetRef` to inline base64
- [ ] Privacy flag prevents cloud model access

---

### Feature 3.2: Conversation Structure

**Problem**: Current model doesn't support parallel model responses, multi-step tool interactions, or comparing alternatives.

**Solution**: Conversations as sequences of turns, each with alternative spans containing messages.

**Functional Requirements**:
- Conversation contains ordered turns (position in sequence)
- Each turn has one or more spans (alternative responses)
- Each span contains ordered messages (for multi-step flows)
- Span has role (user/assistant) identifying owner
- Message has role for multi-step support (assistant → tool → assistant)
- Message references content block for text
- Tool calls/results stored inline in message

**Use Cases Enabled**:
- Parallel model responses: Multiple spans at same turn, compare them
- Tool interactions: Single span contains assistant → tool_call → tool_result → response
- User edits as alternatives: Edit creates new user span at same turn

**Acceptance Criteria**:
- [ ] Create conversation with turns and spans
- [ ] Span contains multiple messages (multi-step flow)
- [ ] Different spans at same turn can have different message counts
- [ ] Messages reference content blocks (text is searchable)
- [ ] Tool calls/results preserved in messages

---

### Feature 3.3: Views and Forking

**Problem**: No way to branch conversations, compare different paths, or edit mid-conversation.

**Solution**: Views select one span per turn, creating named paths through the conversation.

**Functional Requirements**:
- Views select which span to use at each turn
- Main view is default (created with conversation)
- Fork creates new view sharing selections up to fork point
- Span selection affects subsequent context
- Views are cheap (just selection pointers, content not duplicated)

**Use Cases Enabled**:
- Fork conversation: Branch from turn 3, explore different direction
- Edit and splice: New span at turn 3, reuse turns 4-5 from original
- A/B comparison: Two views selecting different spans

**Acceptance Criteria**:
- [ ] Create view for conversation
- [ ] View selects spans, forming coherent path
- [ ] Fork view at turn N shares turns 1..(N-1)
- [ ] Forked view can select different spans after fork point
- [ ] Get view path returns selected span messages in order

---

### Feature 3.4: Document Structure

**Problem**: Documents are flat with no structure. Can't organize sections or track where content came from.

**Solution**: Documents with hierarchical tabs, each tab having its own revision history.

**Functional Requirements**:
- Document contains tabs (structural pointers to content)
- Tabs can be nested (sub-tabs)
- Each tab has independent revision history
- Revisions reference content blocks (text is searchable, deduplicated)
- Track document source (user created, AI generated, imported, promoted from message)
- Promote message to document (reuses content block)

**Use Cases Enabled**:
- Multi-section documents: Overview tab, Details tab with sub-tabs
- Version history per section: Revert just one tab
- AI → Document pipeline: Save assistant response as document

**Acceptance Criteria**:
- [ ] Create document with initial tab
- [ ] Add nested tabs (hierarchy)
- [ ] Commit creates new revision for tab
- [ ] Branch revision from non-head
- [ ] Checkout moves tab to specific revision
- [ ] Promote message to document (reuses content block)

---

### Feature 3.5: Collections

**Problem**: No unified way to organize content across types. Can't create project views, task lists, or bookmarks.

**Solution**: Collections as a structural layer over any entity, with schema hints for UI and indexed fields for queries.

**Functional Requirements**:
- Collection items can reference any entity (document, conversation, content block, other collection)
- Items form tree structure (nested folders)
- Items have position (ordered)
- Items can have tags (cross-cutting organization)
- Items can have typed fields (for table/kanban views)
- Schema hints tell UI what fields to expect (advisory, not enforced)
- For document items: frontmatter is source of truth, fields are cached index

**Use Cases Enabled**:
- Project folder: Documents and conversations grouped
- Task list: Items with status, priority, due date fields
- Bookmarks: Mixed entity types in one list
- Kanban board: Items grouped by status field

**Acceptance Criteria**:
- [ ] Create collection with items referencing different entity types
- [ ] Nested items (tree structure)
- [ ] Reorder items (move within/between parents)
- [ ] Tag items and query by tag
- [ ] Set fields and query/filter by field value
- [ ] Schema hint guides UI field display

---

### Feature 3.6: Cross-References

**Problem**: No way to link content across conversations, documents, collections. No backlinks.

**Solution**: Generic reference system between any entity types with automatic backlink tracking.

**Functional Requirements**:
- Reference from any entity to any entity
- Optional relation type (cites, derived_from, etc.)
- Backlinks auto-computed (who references this?)
- Support @-mention syntax in content

**Use Cases Enabled**:
- Document cites conversation: "Generated from [chat X]"
- Message references document: "See @api-design for details"
- Backlinks panel: "Referenced by 3 conversations, 1 document"

**Acceptance Criteria**:
- [ ] Create reference between entities
- [ ] Query outgoing references from entity
- [ ] Query incoming references (backlinks) to entity
- [ ] References survive entity updates
- [ ] Delete reference when source entity deleted

---

### Feature 3.7: Temporal Queries

**Problem**: LLM needs activity context ("what have I been working on?") but no efficient time-based queries.

**Solution**: Indexed timestamps enabling time-range queries with summarization for LLM context.

**Functional Requirements**:
- Query content by time range (last hour, last day, last week)
- Group by entity type (conversations, documents)
- Generate activity summary for LLM injection
- Configurable detail level (brief, detailed)

**Use Cases Enabled**:
- "Summarize my work from last week"
- "What topics have I been exploring?"
- Proactive assistant: "I noticed you've been working on X..."

**Acceptance Criteria**:
- [ ] Query messages/content in time range
- [ ] Group results by conversation/document
- [ ] Generate markdown summary of activity
- [ ] Summary respects token budget

---

### Feature 3.8: Session Integration

**Problem**: Engine session uses old conversation model. Need to connect to new structure.

**Solution**: Adapter connecting SqliteSession to Turn/Span/Message model.

**Functional Requirements**:
- `commit()` creates turn + span + messages
- Message text stored via content block store
- `commit_parallel_responses()` creates one turn with multiple spans
- `open_conversation()` loads main view's selected spans
- Existing session API preserved (engine unchanged)

**Acceptance Criteria**:
- [ ] Send message → creates turn, span, message, content block
- [ ] Load conversation → returns messages from main view path
- [ ] Parallel responses → multiple spans at same turn
- [ ] Engine works without modification

---

### Feature 3.9: Migration and Cleanup

**Problem**: Legacy tables (threads, span_sets, spans, span_messages) need removal.

**Solution**: Remove old schema after session integration verified.

**Functional Requirements**:
- Verify all functionality works with new model
- Drop legacy tables
- Clean up old code paths

**Acceptance Criteria**:
- [ ] All tests pass with new model
- [ ] Legacy tables dropped
- [ ] No references to old table names in code

---

## Key Design Decisions

### Spans vs Messages

**Span** = an autonomous flow owned by one party (user or assistant)
**Message** = individual content within a span

A single assistant span can contain: thinking → tool_call → tool_result → response

This enables parallel model comparison where different models produce different numbers of messages.

### Content Deduplication

All text goes through content blocks. Same text = same hash = stored once.

Benefits:
- Deduplication across messages, documents, revisions
- Unified full-text search
- Cross-referencing ("as I said in message X")
- Origin tracking (who created, derived from what)

### Collections as Meta-Structure

Collections don't own content - they organize references to it.

For document items, frontmatter is the source of truth for fields. `item_fields` is a cached index regenerated on content change.

---

## Related Documents

- [PLAN.md](PLAN.md) - Detailed implementation plan with schema and API
- [UNIFIED_CONTENT_MODEL.md](../../design/UNIFIED_CONTENT_MODEL.md) - Design document
- [HOOK_SYSTEM.md](../../design/HOOK_SYSTEM.md) - Future extension points
