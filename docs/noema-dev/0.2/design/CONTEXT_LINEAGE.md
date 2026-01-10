# Context Lineage Design

**Status:** Draft
**Created:** 2026-01-10
**Related:** IDEAS #1, #2

---

## Problem

Current "conversation" model is linear. We need to support:

1. Branching conversations
2. Parallel model responses
3. Subagent contexts (scoped views)
4. Agent-to-agent communication
5. Editable/recomputable history
6. Hierarchical summarization

## Core Insight

**Separate content from structure.**

| Layer | Contains | Properties |
|-------|----------|------------|
| Content | Messages, documents, assets | Immutable, deduplicated, content-addressed |
| Structure | Contexts, spans, threads | Mutable, cheap to fork, tracks lineage |

---

## Content Layer

Universal content primitive with intrinsic lineage:

```
ContentBlock {
    id: ContentHash,              // SHA-256, content-addressed
    content_type: ContentType,    // text/markdown, image/png, etc.
    body: Bytes,
    origin: ContentOrigin,
    created_at: Timestamp,
}

ContentOrigin {
    kind: user | assistant | system | import | tool,
    user_id: Option<UserId>,      // which user (multi-user)
    model_id: Option<ModelId>,    // which model (if AI)
    source_id: Option<String>,    // external ID (google doc, url)
    parent_content_id: Option<ContentHash>,  // if derived/edited
}
```

**Usage tables** map content to contexts:

- `MessageUsage` - content as conversation message (+ role)
- `DocumentRevisionUsage` - content as document version (+ revision number)
- `AssetUsage` - content as asset (+ filename, mime type)

Same content can appear in multiple places.

---

## Structure Layer

### Context

First-class entity for scoped view with lineage:

```
Context {
    id: ContextId,
    parent_context: Option<ContextId>,  // lineage
    tip_span: SpanId,                   // current position
    summary: Option<Summary>,           // compressed representation
    metadata: ContextMetadata,          // overrides
}
```

### Existing Structure (spans)

Current schema handles alternatives at a position:

```
Thread → SpanSet (position) → Span (alternative) → MessageUsage → ContentBlock
```

Context adds **scoped views with lineage** on top.

---

## Operations

| Operation | What it does | Cost |
|-----------|--------------|------|
| `extend(content)` | New span + context | O(1) structure |
| `fork()` | New context, same tip | O(1) |
| `subset(filter)` | Filtered view | O(n) refs |
| `summarize()` | Generate summary | LLM call |

---

## Capability Mapping

| Capability | How it works |
|------------|--------------|
| **Branching** | `fork()` - new context, shared content |
| **Parallel models** | Multiple spans in same span_set |
| **Subagent** | Child context with `parent_context`, gets subset + summary bubbles up |
| **Agent-to-agent** | Separate contexts, shared content via message passing |
| **Edit & replay** | Fork from edit point, new span with edited content |
| **Summarization** | Context holds summary, can expand to full |

---

## Schema Additions

```sql
CREATE TABLE content_blocks (
    id TEXT PRIMARY KEY,                    -- SHA-256
    content_type TEXT NOT NULL,
    body BLOB NOT NULL,
    origin_kind TEXT NOT NULL,
    origin_user_id TEXT REFERENCES users(id),
    origin_model_id TEXT,
    origin_source_id TEXT,
    origin_parent_id TEXT REFERENCES content_blocks(id),
    created_at INTEGER NOT NULL
);

CREATE TABLE contexts (
    id TEXT PRIMARY KEY,
    parent_context_id TEXT REFERENCES contexts(id),
    tip_span_id TEXT REFERENCES spans(id),
    summary_text TEXT,
    summary_embedding BLOB,
    system_prompt_override TEXT,
    model_override TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE message_usages (
    id TEXT PRIMARY KEY,
    content_id TEXT REFERENCES content_blocks(id),
    role TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
```

---

## Open Questions

1. Garbage collection for orphaned content?
2. Large blobs: BlobStore vs content_blocks?
3. Search: unified index or per-usage-type?
4. Summary invalidation when context extends?

---

## Migration Path

1. Extract content to `content_blocks`
2. Add `contexts` table
3. Enable forking, subagents, summarization

Each phase backward-compatible.
