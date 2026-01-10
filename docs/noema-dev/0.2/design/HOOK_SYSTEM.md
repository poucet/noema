# Hook System

**Status:** Draft
**Created:** 2026-01-10
**Related:** [UNIFIED_CONTENT_MODEL.md](UNIFIED_CONTENT_MODEL.md), IDEAS #5, #6, #7, #8

---

## Overview

A data-driven, extensible hook system where patterns, actions, and events are all content. No hardcoded event types or handler logic - everything is stored as ContentBlocks and interpreted at runtime.

---

## Core Principle

**Hooks are content, not code.**

- Event types are strings (extensible without code changes)
- Patterns are ContentBlocks (describe what to match)
- Actions are ContentBlocks (describe what to do)
- Events themselves are logged as ContentBlocks

---

## Model

```
┌─────────────────────────────────────────────────────────────┐
│                      EVENT SOURCES                          │
│                                                             │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │
│  │ Entity   │  │ Temporal │  │ Render   │  │ External │   │
│  │ Lifecycle│  │ Scheduler│  │ Pipeline │  │ Triggers │   │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘   │
│       │             │             │             │          │
│       └─────────────┴──────┬──────┴─────────────┘          │
│                            ▼                                │
│                     ┌──────────┐                           │
│                     │  Events  │ (logged as content)       │
│                     └────┬─────┘                           │
└──────────────────────────┼──────────────────────────────────┘
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                      HOOK ENGINE                            │
│                                                             │
│  ┌─────────────────┐    ┌─────────────────┐                │
│  │ Pattern Matcher │───▶│ Action Executor │                │
│  │ (interprets     │    │ (interprets     │                │
│  │  pattern content)    │  action content)│                │
│  └─────────────────┘    └─────────────────┘                │
│           ▲                      │                          │
│           │                      ▼                          │
│  ┌─────────────────────────────────────────┐               │
│  │              Hook Registry              │               │
│  │  (pattern_content_id, action_content_id)│               │
│  └─────────────────────────────────────────┘               │
└─────────────────────────────────────────────────────────────┘
```

---

## Event Types

Event types are **strings**, not enums. Convention-based, extensible:

```
# Entity lifecycle
entity.created.{type}     # entity.created.message, entity.created.document
entity.updated.{type}
entity.deleted.{type}

# Temporal
temporal.idle.{type}      # temporal.idle.conversation
temporal.timeout.{name}   # temporal.timeout.daily-checkin
temporal.interval.{name}
temporal.scheduled.{name}

# Render pipeline
render.before.{target}    # render.before.llm, render.before.ui
render.after.{target}

# Conversation
conversation.started
conversation.turn.complete
conversation.context.overflow

# Import/Export
import.before.{format}
import.after.{format}
export.before.{format}
export.after.{format}

# Custom (user-defined)
custom.{namespace}.{name}
```

New event types are added by:
1. Code emitting new `event_type` strings
2. Hooks pattern-matching on them

---

## Schema

### Events Table

Events are logged with their payload as content:

```sql
CREATE TABLE events (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,           -- Extensible string
    payload_content_id TEXT,            -- ContentBlock: event details (JSON/YAML)
    source_entity_type TEXT,            -- What entity triggered this
    source_entity_id TEXT,
    timestamp INTEGER NOT NULL,
    FOREIGN KEY (payload_content_id) REFERENCES content_blocks(id)
);

CREATE INDEX idx_events_type_time ON events(event_type, timestamp);
CREATE INDEX idx_events_source ON events(source_entity_type, source_entity_id);
```

### Hooks Table

Hooks bind patterns to actions:

```sql
CREATE TABLE hooks (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    pattern_content_id TEXT NOT NULL,   -- ContentBlock: event pattern
    action_content_id TEXT NOT NULL,    -- ContentBlock: action spec
    priority INTEGER DEFAULT 0,
    enabled BOOLEAN DEFAULT TRUE,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (pattern_content_id) REFERENCES content_blocks(id),
    FOREIGN KEY (action_content_id) REFERENCES content_blocks(id)
);

CREATE INDEX idx_hooks_enabled ON hooks(enabled, priority);
```

### Temporal Triggers Table

Temporal sources that emit events:

```sql
CREATE TABLE temporal_triggers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    trigger_type TEXT NOT NULL,         -- schedule, idle, timeout
    config_content_id TEXT NOT NULL,    -- ContentBlock: trigger config
    emits_event_type TEXT NOT NULL,     -- What event type to emit
    enabled BOOLEAN DEFAULT TRUE,
    last_fired_at INTEGER,
    next_fire_at INTEGER,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (config_content_id) REFERENCES content_blocks(id)
);

CREATE INDEX idx_temporal_next ON temporal_triggers(enabled, next_fire_at);
```

---

## Pattern Language

Patterns are ContentBlocks. Start simple, evolve over time.

### Simple Match

```yaml
event_type: "entity.created.message"
```

### With Filters

```yaml
event_type: "entity.created.message"
filter:
  entity.conversation_id: "conv-123"
  # Or use tags, fields, etc.
```

### Wildcards

```yaml
event_type: "entity.created.*"
```

### Temporal Patterns

```yaml
event_type: "temporal.idle"
config:
  entity_type: conversation
  duration: "24h"
```

### Compound Patterns

```yaml
any:
  - event_type: "entity.created.message"
  - event_type: "entity.updated.message"
```

```yaml
all:
  - event_type: "entity.created.message"
  - filter:
      entity.tags: ["important"]
```

### Future: Natural Language Patterns

```yaml
natural_language: "When a message is created in any conversation tagged 'journal'"
```

(Interpreted by LLM at pattern-match time)

---

## Action Language

Actions are ContentBlocks. Describe what to do, not how.

### Create Entity

```yaml
action: create_entity
entity_type: message
template: |
  Good morning! Reflecting on yesterday:
  {{summarize(conversation, last=10)}}

  What's on your mind today?
target:
  conversation_id: "{{event.source_entity_id}}"
```

### Transform Content

```yaml
action: transform
pipeline:
  - evaluate_typst
  - inject_query_results
  - summarize_if_long
```

### Enqueue for Async

```yaml
action: enqueue
queue: journal-extractor
payload:
  message_id: "{{event.source_entity_id}}"
  conversation_id: "{{event.payload.conversation_id}}"
```

### Update Entity

```yaml
action: update_entity
entity_type: "{{event.source_entity_type}}"
entity_id: "{{event.source_entity_id}}"
fields:
  last_activity_at: "{{now()}}"
```

### Trigger Another Hook

```yaml
action: trigger_event
event_type: "custom.cascade.step2"
payload:
  original_event: "{{event.id}}"
```

### Notify/Log

```yaml
action: log
level: info
message: "User idle for 24h in conversation {{event.source_entity_id}}"
```

---

## Temporal Triggers

Temporal triggers are event sources, not hooks themselves.

### Schedule-based (Cron)

```yaml
trigger_type: schedule
config:
  cron: "0 9 * * *"          # Daily at 9am
emits: "temporal.scheduled.morning-checkin"
```

### Idle-based

```yaml
trigger_type: idle
config:
  watch_entity_type: conversation
  idle_duration: "24h"
  # Optionally scope to specific entities
  filter:
    tags: ["active"]
emits: "temporal.idle.conversation"
payload_template:
  conversation_id: "{{entity.id}}"
  idle_since: "{{entity.last_activity_at}}"
```

### Timeout (One-shot)

```yaml
trigger_type: timeout
config:
  fire_at: "2026-01-15T10:00:00Z"
emits: "temporal.timeout.reminder"
payload_template:
  reminder: "Follow up on project X"
```

---

## Use Cases Enabled

| Idea | Pattern | Action |
|------|---------|--------|
| **#5 Dynamic Typst** | `render.before.llm` | Transform: evaluate Typst, inject query results |
| **#6 Proactive check-ins** | `temporal.idle.conversation` | Create: message from check-in template |
| **#7 Context management** | `conversation.context.overflow` | Transform: summarize older messages |
| **#8 Auto-journaling** | `entity.created.message` | Enqueue: extract insights to journal |
| **#4 Filesystem sync** | `entity.updated.document` | Custom: write to filesystem |
| **#1 Access control** | `entity.*.before` (future) | Validate: check permissions |

---

## Implementation Phases

### Phase 1: Event Infrastructure

- Add `events` table
- Emit events from entity lifecycle (create/update/delete)
- Basic event logging and querying

### Phase 2: Hook Registry

- Add `hooks` table
- Simple pattern matching (exact event_type)
- Basic action execution (log, enqueue)

### Phase 3: Temporal Triggers

- Add `temporal_triggers` table
- Scheduler service reads triggers, emits events
- Idle detection based on `updated_at` timestamps

### Phase 4: Rich Patterns & Actions

- Wildcard and compound patterns
- Template expansion in actions
- Transform pipelines for render hooks

### Phase 5: User-Defined Hooks

- UI for creating/editing hooks
- Hook versioning (pattern/action as Documents)
- Natural language pattern interpretation

---

## Open Questions

1. **Sync vs Async**: Which actions block the triggering operation?
2. **Error handling**: What happens when an action fails?
3. **Recursion**: How to prevent infinite hook chains?
4. **Ordering**: How to handle multiple hooks matching same event?
5. **Permissions**: Who can create hooks that affect others' content?

---

## Summary

| Concept | Storage | Interpretation |
|---------|---------|----------------|
| Event types | Strings in `events.event_type` | Convention-based matching |
| Patterns | ContentBlocks | Pattern matcher interprets |
| Actions | ContentBlocks | Action executor interprets |
| Hooks | Registry binding pattern → action | Hook engine coordinates |
| Temporal | ContentBlocks in `temporal_triggers` | Scheduler interprets |

The system is fully data-driven. New event types, patterns, and actions can be added without code changes - only new interpreters for new pattern/action languages require code.
