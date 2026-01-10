# Phase 3: Unified Content Model

## Overview

Phase 3 establishes the foundational data model where everything is a node with properties, relations, and views. This architectural work enables all future features.

## Task Table

| Status | Pri | # | Feature | Files |
|--------|-----|---|---------|-------|
| ⬜ | P0 | 29a | Core node system (base types, properties, metadata) | node/mod.rs, storage/node.rs |
| ⬜ | P0 | 29b | Container nodes (workspace, tag/folder, database) | node/container.rs |
| ⬜ | P1 | 29c | Content nodes (conversation, thread, message, document, span) | node/content.rs |
| ⬜ | P1 | 29d | Structured data (schemas, column types, templates, formulas) | node/schema.rs |
| ⬜ | P1 | 29e | Relations system (parent/child, references, backlinks) | node/relations.rs |
| ⬜ | P1 | 29f | UI views (list/table, tree, board, graph, timeline) | components/views/ |
| ⬜ | P2 | 29g | Agent nodes and context injection | node/agent.rs |
| ⬜ | P2 | 30 | Import/export and data portability | import/, export/ |

Status: ⬜ todo, 🔄 in-progress, ✅ done, 🚫 blocked, ⏸️ deferred

---

## Feature Details

### Feature 29a: Core Node System

**Problem**: Separate systems for conversations, documents, tags with duplicated logic.

**Solution**: Base node type with common properties and metadata.

**Node Base**:
- `id`: UUID
- `type`: node type discriminator
- `properties`: typed key-value map
- `created_at`, `updated_at`: timestamps
- `embedding`: vector for semantic search
- `summary`: auto-generated summary

**Acceptance Criteria**:
- [ ] Node base type defined with all core fields
- [ ] Property system with typed values
- [ ] SQLite storage for nodes
- [ ] CRUD operations for nodes

---

### Feature 29b: Container Nodes

**Problem**: Need organizational structures to hold content.

**Solution**: Workspace, tag/folder, and database container types.

**Types**:
- **Workspace**: top-level container, user's root
- **Tag/Folder**: hierarchical organization, supports nesting
- **Database**: schema-defined collection with typed columns

**Acceptance Criteria**:
- [ ] Workspace node type
- [ ] Tag/folder with nesting support
- [ ] Database with schema definition

---

### Feature 29c: Content Nodes

**Problem**: Content types need unified representation.

**Solution**: Conversation, thread, message, document, span as node types.

**Acceptance Criteria**:
- [ ] Conversation node with threads
- [ ] Message nodes with content blocks
- [ ] Document nodes with revisions
- [ ] Span nodes for annotations

---

### Feature 29d: Structured Data

**Problem**: Need schema-defined properties and computed values.

**Solution**: Column types, templates, and formula system.

**Acceptance Criteria**:
- [ ] Column type definitions
- [ ] Formula evaluation
- [ ] Template system

---

### Feature 29e: Relations System

**Problem**: Need connections between nodes.

**Solution**: Parent/child, references, and backlinks.

**Acceptance Criteria**:
- [ ] Parent/child relations
- [ ] Reference links with backlink tracking
- [ ] Relation queries

---

### Feature 29f: UI Views

**Problem**: Different ways to view and interact with nodes.

**Solution**: List, table, tree, board, graph, timeline views.

**Acceptance Criteria**:
- [ ] List/table view component
- [ ] Tree view component
- [ ] Board (kanban) view
- [ ] View switching

---

### Feature 29g: Agent Nodes

**Problem**: Agents need first-class representation.

**Solution**: Agent nodes with context injection.

**Acceptance Criteria**:
- [ ] Agent node type
- [ ] Context injection from linked nodes
- [ ] Agent templates

---

### Feature 30: Import/Export

**Problem**: Data portability needed.

**Solution**: JSON, Markdown, CSV export; import from various sources.

**Acceptance Criteria**:
- [ ] JSON full-fidelity export
- [ ] Markdown export for documents
- [ ] Import from files

---

## Key Files Reference

See design doc: [design/UNIFIED_CONTENT_MODEL.md](../../design/UNIFIED_CONTENT_MODEL.md)

### New Modules (to create)
- `noema-core/src/node/` - Node type definitions
- `noema-core/src/storage/node.rs` - Node storage
- `noema-desktop/src/components/views/` - View components
