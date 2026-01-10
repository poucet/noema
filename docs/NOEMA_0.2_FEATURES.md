# Noema 0.2 Feature Wave Plan

## Summary

This plan covers a major feature wave for Noema 0.2, organized into 7 phases. Key architectural decisions:

1. **Unified content model** - everything is a node with properties, relations, and views
2. **Full embedding infrastructure** enabling semantic search and RAG
3. **Auto-summarization** for meaningful conversation names and quick previews
4. **Single source of truth** for versioning across all config files

---

## Feature Overview (Sorted by Phase)

### Phase 1: Quick Wins
| Done | Pri | # | Feature | Complexity | Impact |
|------|-----|---|---------|------------|--------|
| [ ] | P0 | 32 | Private content flag (blocks non-local models) | Low | High |
| [ ] | P1 | 3 | Model metadata display (context window, provider) | Low | Medium |
| [x] | P1 | 4 | Local vs non-local model indicator icon | Low | Medium |
| [ ] | P1 | 31 | Copy raw markdown from assistant responses | Low | Medium |
| [ ] | P1 | 33 | Toggle to disable tools (for models without tool support) | Low | Medium |
| [ ] | P1 | 34 | Toggle to disable audio/image input (placeholder + toggle) | Low | Medium |
| [ ] | P2 | 2 | Truncate long model names (preserve star icon) | Low | Low |

### Phase 2: Core UX (Model-Independent)
| Done | Pri | # | Feature | Complexity | Impact |
|------|-----|---|---------|------------|--------|
| [ ] | P0 | 26 | @-mention file search beyond initial list | Low | Medium |
| [ ] | P1 | 28 | Parallel conversations with status indicators | Medium | High |
| [ ] | P2 | 5 | Copy-paste markdown into ChatInput | Medium | Medium |
| [ ] | P2 | 18 | Live markdown/Typst rendering with math notation | Medium | Medium |
| [ ] | P3 | 27 | Fix Google Docs import search | Low | Low |

### Phase 3: Unified Content Model
| Done | Pri | # | Feature | Complexity | Impact |
|------|-----|---|---------|------------|--------|
| [ ] | P0 | 29a | Core node system (base types, properties, metadata) | High | Very High |
| [ ] | P0 | 29b | Container nodes (workspace, tag/folder, database) | High | Very High |
| [ ] | P1 | 29c | Content nodes (conversation, thread, message, document, span) | High | Very High |
| [ ] | P1 | 29d | Structured data (schemas, column types, templates, formulas) | Medium | High |
| [ ] | P1 | 29e | Relations system (parent/child, references, backlinks) | Medium | High |
| [ ] | P1 | 29f | UI views (list/table, tree, board, graph, timeline) | High | High |
| [ ] | P2 | 29g | Agent nodes and context injection | High | Very High |
| [ ] | P2 | 30 | Import/export and data portability | Medium | High |

### Phase 4: Content Model Features (Post-Unification)
| Done | Pri | # | Feature | Complexity | Impact |
|------|-----|---|---------|------------|--------|
| [ ] | P1 | 1 | Undo delete (soft delete via node status) | Low | High |
| [ ] | P1 | 10 | Per-conversation system prompts (node property) | Low | High |
| [ ] | P1 | 12 | Auto-naming via summarizer (node metadata) | Medium | High |
| [ ] | P1 | 13 | Summaries for all nodes | Medium | Medium |
| [ ] | P1 | 17 | Document editing with revision history | Medium | High |
| [ ] | P2 | 6 | Drag-and-drop reordering (node sort_order) | Low | Low |

### Phase 5: Organization + Search
| Done | Pri | # | Feature | Complexity | Impact |
|------|-----|---|---------|------------|--------|
| [ ] | P0 | 21 | Custom skills and slash commands | Medium | High |
| [ ] | P1 | 14 | Embedding infrastructure + semantic search | High | Very High |
| [ ] | P1 | 7 | Document hierarchy via nested tags | High | High |
| [ ] | P1 | 11 | Conversation hierarchy via nested tags | High | High |
| [ ] | P2 | 16 | Wiki-style cross-linking (conversations ↔ documents) | Medium | Medium |
| [ ] | P2 | 22 | External integrations (Notion, Google Calendar, etc.) | Medium | Medium |

### Phase 6: RAG + Memories
| Done | Pri | # | Feature | Complexity | Impact |
|------|-----|---|---------|------------|--------|
| [ ] | P1 | 8 | Conversation memories (hybrid auto/manual) | High | Very High |
| [ ] | P1 | 15 | Full RAG (Retrieval-Augmented Generation) | High | Very High |
| [ ] | P1 | 19 | MCP coding agent tools (file edit, terminal, etc.) | High | Very High |
| [ ] | P2 | 9 | Documentation generation with versioning | High | Medium |

### Phase 7: Agentic + Multimodal Features
| Done | Pri | # | Feature | Complexity | Impact |
|------|-----|---|---------|------------|--------|
| [ ] | P0 | 23 | Audio models (STT/TTS integration) | Medium | High |
| [ ] | P1 | 20 | Multi-agent/sub-agent conversations | Very High | Very High |
| [ ] | P2 | 24 | Image generation models (local/remote) | Medium | Medium |
| [ ] | P2 | 25 | PDF extraction and processing | Medium | Medium |

### Future / Out of Scope for 0.2
| # | Feature | Notes |
|---|---------|-------|
| - | Noema Web (browser version) | Requires noema-backend extraction |
| - | Cloud sync / multi-device | Requires backend service |

---

## Phase 1: Quick Wins (Model Display Improvements)

### Feature 2: Truncate Long Model Names

**Problem**: Long model names (e.g., `llama-3.3-70b-instruct-q4_K_M`) push star icon off-screen.

**Solution**: CSS truncation with ellipsis, star icon outside truncated container.

**File**: `noema-desktop/src/components/ModelSelector.tsx`

---

### Feature 3: Model Metadata Display

**Problem**: Users want to see context window size and provider info.

**Solution**: Secondary info line below model name showing `{contextWindow}K tokens • {provider}`.

**File**: `noema-desktop/src/components/ModelSelector.tsx`

---

### Feature 4: Local vs Non-Local Model Icon

**Problem**: Users want visual indication of privacy (local vs cloud).

**Solution**: Icon next to provider name.
- **Local**: `ollama`, `llama.cpp`, `localai`, `lmstudio` → laptop/shield icon
- **Cloud**: `anthropic`, `openai`, `gemini`, `openrouter`, `groq` → cloud icon

---

### Feature 31: Copy Raw Markdown from Assistant Responses

**Problem**: Users want to copy the raw markdown source of assistant responses (for pasting into docs, code, etc.) rather than the rendered HTML.

**Solution**: Add copy button on hover over assistant messages with option to copy raw markdown.
- Copy icon appears on hover in message header/toolbar
- Click copies raw markdown to clipboard
- Brief toast confirmation: "Copied to clipboard"

**File**: `noema-desktop/src/components/Message.tsx`

---

### Feature 32: Private Content Flag

**Problem**: Users want to ensure sensitive content (conversations, documents, etc.) is never sent to cloud/non-local models.

**Solution**: Add "private" flag to content items. Private content blocks non-local model usage.
- Toggle in conversation/document settings
- Visual indicator (lock icon) on private items
- When attempting to use cloud model with private content:
  - Show warning dialog explaining data will leave device
  - Require explicit confirmation to proceed
  - Option to switch to local model instead
- Works with Feature 4 (local vs non-local indicator) to show which models are safe

**Behavior**:
- Private conversation → blocks cloud models for that conversation
- Private document attached → blocks cloud models for that message
- Inherits: private tag → all children are private

**Files**:
- `noema-desktop/src/components/ConversationSettings.tsx`
- `noema-core/src/engine.rs` (model selection validation)

---

### Feature 33: Toggle to Disable Tools

**Problem**: Some models don't support tool/function calling. Users need an easy way to disable tools when using such models.

**Solution**: Add toggle at bottom of chat input area to enable/disable tools.
- Toggle shows current state: "Tools: On/Off"
- When off, MCP tools are not sent to the model
- Per-conversation setting (persisted)
- Visual indicator when tools are disabled

**File**: `noema-desktop/src/components/ChatInput.tsx`

---

### Feature 34: Toggle to Disable Audio/Image Input

**Problem**: Some models don't support multimodal input (images, audio). When users try to attach media, it either fails or gets ignored.

**Solution**: Add toggles to disable audio/image input with placeholder UI.
- When model lacks `Vision` capability, image input is disabled
- When model lacks `AudioInput` capability, audio input is disabled
- Show placeholder instead of input button: "Model doesn't support images"
- Easy toggle to force-enable (for testing or model capability updates)
- Per-conversation override possible

**File**: `noema-desktop/src/components/ChatInput.tsx`

---

## Phase 2: Core UX Improvements

### Feature 1: Undo Delete Conversation/Document

**Problem**: Accidental deletion is permanent.

**Solution**: Soft delete with toast notification + undo button.

**Schema Changes**:
```sql
ALTER TABLE conversations ADD COLUMN deleted_at INTEGER;
ALTER TABLE documents ADD COLUMN deleted_at INTEGER;
```

**Key Changes**:
- `delete_*()` → SET deleted_at = now()
- `restore_*(id)` → SET deleted_at = NULL
- All queries add: `WHERE deleted_at IS NULL`
- New Toast component with undo button, auto-dismiss after 10s

---

### Feature 5: Copy-Paste Markdown into ChatInput

**Problem**: Pasting formatted content loses markdown structure.

**Solution**: Convert HTML clipboard to markdown on paste using `turndown` package.

**File**: `noema-desktop/src/components/ChatInput.tsx`

---

### Feature 6: Drag-and-Drop Conversation Reordering

**Problem**: Conversations are chronological only.

**Solution**: Add `sort_order` column + drag-drop UI with `@dnd-kit`.

**Schema Change**:
```sql
ALTER TABLE conversations ADD COLUMN sort_order INTEGER DEFAULT 0;
```

---

### Feature 10: Per-Conversation System Prompts

**Problem**: `system_prompt` column exists but is unused.

**Solution**: Expose via UI (settings icon in conversation header) and inject into LLM calls.

---

### Feature 12: Auto-Naming Conversations via Summarizer

**Problem**: Conversations default to "New Conversation".

**Solution**: After first assistant response, generate 3-6 word title via LLM. Manual rename takes precedence.

**New Module**: `noema-core/src/summarizer.rs`

---

### Feature 13: Summaries for Conversations and Documents

**Problem**: Long conversations/documents need quick previews.

**Solution**: Generate and store summaries. Display on hover/expand in list views.

**Schema** (conversations already has these, add to documents):
```sql
ALTER TABLE documents ADD COLUMN summary_text TEXT;
ALTER TABLE documents ADD COLUMN summary_embedding BLOB;
```

---

### Feature 17: Document Editing with Full Revision History

**Problem**: Documents can't be edited; no version control.

**Solution**: Full editing for `user_created` and `ai_generated` docs. `google_drive` stays read-only.

**Existing Schema**: `document_revisions` table already exists (see STORAGE.md).

**Key Features**:
- Auto-save with debounce
- Revision history sidebar
- Diff view between versions
- Restore previous versions

---

### Feature 18: Live Markdown/Typst Rendering

**Problem**: No live preview; math notation poorly supported.

**Solution**: Split view editor with live preview. Support both Markdown+KaTeX and Typst.

**Why Typst**: Modern syntax, faster compilation, better error messages, supports LaTeX math.

---

### Feature 26: @-Mention File Search

**Problem**: When typing `@` in ChatInput, only initially loaded files are shown. Files not in the initial list can't be found.

**Solution**: Add search/filter capability to the @-mention dropdown so users can find any file.

---

### Feature 27: Fix Google Docs Import Search

**Problem**: Search functionality in the Google Docs import screen is broken.

**Solution**: Debug and fix the search/filter in the docs import modal.

---

### Feature 28: Parallel Conversations with Status Indicators

**Problem**: When AI is responding in one conversation, user can't switch to another conversation and continue working. No visual indication of conversation state.

**Solution**:
- Allow switching conversations while AI is streaming a response
- Show status indicators in conversation list: busy (spinner), has new messages (badge)
- Background conversations continue processing independently

---

## Phase 3: Unified Content Model

**Problem**: Conversations, documents, tags, databases are separate systems with duplicated hierarchy/relation logic. No unified way to organize, link, and view all content.

**Solution**: Unified data model where everything is a node.

---

### Feature 29a: Core Node System

Base infrastructure for all content types.

**Node Base**:
- `id`: UUID
- `type`: node type discriminator
- `properties`: typed key-value map
- `created_at`, `updated_at`: timestamps
- `embedding`: vector for semantic search
- `summary`: auto-generated summary

**Property Types**: text, number, date, select, multi-select, checkbox, relation, formula

---

### Feature 29b: Container Nodes

Organizational structures that hold other nodes.

- **Workspace**: top-level container, user's root
- **Tag/Folder**: hierarchical organization, supports nesting and multi-tagging
- **Database**: schema-defined collection with typed columns

---

### Feature 29c: Content Nodes

Leaf items that hold actual content.

- **Conversation**: chat session container
- **Thread**: sub-conversation or branch within conversation
- **Message**: single chat message (user/assistant/system)
- **Document**: markdown/Typst content with revisions
- **Span**: highlighted/annotated section within content
- **Row**: structured record in a database

---

### Feature 29d: Structured Data with Formulas

Schema-defined properties and dynamic content.

**Column Types**:
- Basic: text, number, date, checkbox
- Selection: select, multi-select
- Relations: relation (link to other nodes)
- Computed: formula, rollup, count

**Formulas & Dynamic Content**:
- `{{count:tag:todos WHERE done=false}}` - count incomplete todos
- `{{sum:column:effort}}` - sum a numeric column
- `{{query:type:feature | count}}` - count features
- Formulas work in documents AND database columns

**Built-in Templates**:
- **Todo list**: checkbox + task + due + priority + tags
- **Feature tracker**: # + feature + pri + complexity + impact + phase + `{{count}}`
- **Reading list**: title + author + status + rating
- **Meeting notes**: date + attendees + agenda + action items
- **Custom**: user-defined schemas

---

### Feature 29e: Relations System

Connections between nodes.

- **Parent/child**: hierarchical containment
- **References**: explicit `[[node:id]]` links
- **Backlinks**: auto-computed incoming references
- **Relations**: typed links (e.g., "blocks", "relates to", "depends on")

---

### Feature 29f: UI Views

Efficient UI models for different use cases.

- **List/Table**: sortable, filterable columns - databases, search results
- **Tree**: hierarchical navigation - tags, folders, threads
- **Board**: kanban-style grouping - todos by status, features by phase
- **Graph**: relationship visualization - backlinks, dependencies
- **Timeline**: chronological view - messages, activity
- **Calendar**: date-based view - todos by due date, meetings
- **Drag-drop**: universal for hierarchy/tagging operations

---

### Feature 29g: Agent Nodes and Context Injection

Agents as first-class content nodes.

**Agent Templates**:
```yaml
name: "Code Reviewer"
system_prompt: |
  You are a code reviewer. Review for bugs, style, best practices.
  Guidelines: {{doc:coding-standards}}
  Feature count: {{count:type:feature WHERE phase="Phase 3"}}
context:
  - query: "tag:current-project AND type:document"
  - node: "doc:architecture-overview"
tools: [read_file, suggest_edit]
```

**Capabilities**:
- Select any nodes as agent context
- Dynamic queries inject live data
- Sub-agents inherit parent context
- Agents organized under projects via tags

---

### Benefits

- Tag can contain any content type
- Database row can link to conversation, document, or agent
- Message can reference a document span
- Single query/filter system across all content
- Formulas provide live counts/sums in documents and tables
- All content manageable from within Noema UI

---

### Feature 30: Import/Export and Data Portability

**Problem**: Data locked in app, no way to backup, share, or migrate content.

**Solution**: Comprehensive import/export system:

**Export Formats**:
- **JSON**: full fidelity export with all metadata and relations
- **Markdown**: documents and conversations as readable files
- **CSV**: database tables for spreadsheet compatibility
- **YAML**: agent templates and prompt libraries

**Export Scope**:
- Single node (document, conversation, agent template)
- Tag with all children (project export)
- Query results (filtered export)
- Full workspace backup

**Import**:
- Drag-drop files into Noema
- Paste content (auto-detect format)
- Import from Notion, Obsidian, markdown folders
- Restore from backup

**Sharing**:
- Export/import agent templates
- Share database schemas
- Prompt library exchange

---

## Phase 4: Content Model Features (Post-Unification)

These features were deferred from Phase 2 because they touch the content model. After the unified node system is in place, they become simpler to implement.

### Feature 1: Undo Delete

Now just a node status flag (`deleted_at` property) with query filter.

---

### Feature 10: Per-Conversation System Prompts

System prompt becomes a node property on conversation nodes.

---

### Feature 12: Auto-Naming via Summarizer

Summary becomes standard node metadata, auto-generated for all nodes.

---

### Feature 13: Summaries for All Nodes

Every node can have an auto-generated summary (stored in node metadata).

---

### Feature 17: Document Editing with Revision History

Document nodes support revisions as child version nodes.

---

### Feature 6: Drag-and-Drop Reordering

Sort order becomes a standard node property, drag-drop uses unified relations system.

---

## Phase 5: Organization + Search

### Features 7 & 11: Hierarchical Tags

**Concept**: Nested tags instead of folders.
- Multi-tagging: One item can have multiple tags
- Hierarchy: Tags can have parent tags
- Ordering: Items have sort order within each tag
- "Untagged" view prevents orphans

**Schema**: New `tags`, `document_tags`, `conversation_tags` tables.

---

### Feature 14: Embedding Infrastructure + Semantic Search

**Concept**: Full embedding pipeline with vector similarity search.

**Components**:
- `EmbeddingModel` trait with Ollama/OpenAI providers
- Text chunking for large content
- Vector storage in SQLite (`embeddings` table)
- Global search bar (Cmd+K) with text/semantic/hybrid modes

---

### Feature 16: Wiki-Style Cross-Linking

**Concept**: `[[link]]` syntax with bidirectional linking.
- `[[doc:Title]]` → Link to document
- `[[conv:Title]]` → Link to conversation
- Backlinks panel shows incoming links

---

### Feature 21: Custom Skills and Slash Commands

**Concept**: Extensible `/command` system like Claude Code.
- Built-in: `/help`, `/new`, `/model`, `/export`
- Prompt skills: `/summarize`, `/explain`, `/review`
- Workflow skills: `/commit`, `/pr`, `/test`
- User-defined custom skills

---

### Feature 22: External Integrations

**Concept**: Plugin-based integration system.
- Notion (read/write)
- Google Calendar (read/write)
- GitHub (read/write)
- Linear, Slack, Obsidian

---

## Phase 6: RAG + Memories

### Feature 8: Conversation Memories

**Concept**: Hybrid auto-suggest + manual approval system.
- LLM extracts candidate memories after responses
- User reviews and approves/dismisses
- Relevant memories injected into future conversations

---

### Feature 9: Documentation Generation

**Concept**: Generate docs from conversations with full edit/version support.
- Select conversation(s) as source
- Choose doc type (README, API docs, Tutorial)
- Edit and regenerate sections

---

### Feature 15: Full RAG

**Concept**: Automatic context retrieval and injection.
- Query → Embed → Search → Rank → Filter → Inject → LLM
- Sources: Documents, past conversations, memories
- Toggle per conversation, show sources used

---

### Feature 19: MCP Coding Agent Tools

**Concept**: MCP server for coding tasks.
- File operations: read, write, edit, delete
- Terminal: run commands, get output
- Git: status, diff, commit, branch
- Code analysis via tree-sitter/LSP

---

## Phase 7: Agentic + Multimodal

### Feature 20: Multi-Agent Conversations

**Concept**: Agent orchestration with delegation and collaboration.
- Primary agent (user-facing) delegates to sub-agents
- Sub-agents: Coder, Reviewer, Researcher, Planner, Testing
- Orchestration: Sequential, parallel, hierarchical, collaborative

---

### Feature 23: Audio Models (STT/TTS)

**Concept**: Pluggable audio model system.
- STT: Whisper (local/API), Deepgram, AssemblyAI
- TTS: Piper (local), OpenAI TTS, ElevenLabs

---

### Feature 24: Image Generation

**Concept**: Local and cloud image generation.
- Providers: Stable Diffusion, DALL-E, Flux
- `/imagine <prompt>` slash command
- Gallery of generated images

---

### Feature 25: PDF Extraction

**Concept**: Full PDF support.
- Text extraction with OCR for scanned PDFs
- Image extraction
- PDF viewing in-app
- Convert to markdown documents

---

## Version Management

**Solution**: Cargo workspace inheritance for Rust crates.

**Root Cargo.toml**:
```toml
[workspace.package]
version = "0.2.0"
```

**Members use**: `version.workspace = true`

**Non-Cargo files** (`package.json`, `tauri.conf.json`): Update manually when bumping version.

---

## Key Files to Modify

| Phase | Files |
|-------|-------|
| 1 | `ModelSelector.tsx` |
| 2 | `ChatInput.tsx`, `engine.rs` (parallel conversations) |
| 3 | New: `noema-core/src/node/`, `storage/node.rs`, `NodeView.tsx`, `TableView.tsx`, `TreeView.tsx`, `BoardView.tsx` |
| 4 | Updates to node properties/metadata for deferred features |
| 5 | New: `storage/tag/`, `storage/vector/`, `embedding/`, `TagTree.tsx`, `SearchBar.tsx` |
| 6 | New: `storage/memory/`, `rag/`, `summarizer.rs`, `MemoriesPanel.tsx` |
| 7 | New: `noema-mcp-coding/`, `AgentPanel.tsx`, audio/image integration |

---

## Implementation Order

```
Prerequisites: Version consolidation

Phase 1 → Phase 2 → Phase 3 (Unified Model) → Phase 4 → Phase 5 → Phase 6 → Phase 7
                              ↓
                        Core Nodes (29a-c)
                              ↓
                   Structured Data (29d) + Relations (29e)
                              ↓
                   UI Views (29f) + Import/Export (30)
                              ↓
                   Deferred Features (1, 6, 10, 12, 13, 17)
                              ↓
                   Embeddings (14) ──→ RAG (15)
                              ↓              ↓
                   Tags (7,11) ───→ Memories (8)
                              ↓
                   Skills (21) ──→ Agents (29g, 19, 20)
```

---

## Verification

- `cargo build --all` - All Rust crates compile
- `npm run build` - Frontend builds
- `cargo tauri build` - Full app builds
- Manual testing per phase as features complete
