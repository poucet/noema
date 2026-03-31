# Phase 3: Discord Polish

**Parent:** [v1.0 Roadmap](../../ROADMAP.md)
**Priority:** P2
**Complexity:** M
**Depends on:** Phases 0, 1A, 1B complete. Phase 2 not required.

---

## Goal

Feature parity for remaining Lumina cogs that weren't covered in earlier phases. Rich Discord UX with embeds, buttons, and autocomplete.

---

## Stages

### 3.1 — Admin + Access Control

**Goal:** Port admin commands. Access control via UCM documents.

**Complexity:** S

**Tasks:**
- [ ] Port `/admin` commands from Python Lumina
- [ ] Access control documents: `type: access_rule`, `role`, `level` frontmatter
- [ ] Role resolution: map Discord roles → access levels via UCM documents
- [ ] Permission checks in Lumina command handlers

**Verify:** Admin can manage roles and permissions via Discord slash commands.

---

### 3.2 — Rich Discord UI

**Goal:** Commands return rich embeds, interactive components.

**Complexity:** M

**Tasks:**
- [ ] Discord embed builder utilities for Lumina
- [ ] Button/select menu interactions for common operations (todo toggle, pagination)
- [ ] Slash command autocomplete for document queries, model selection
- [ ] Poll creation and management

**Verify:** Commands return rich embeds with interactive components instead of plain text.

---

### 3.3 — Server Management

**Goal:** Server-level features from Python Lumina.

**Complexity:** S

**Tasks:**
- [ ] Welcome messages (ties into Phase 1B platform events — may already work)
- [ ] Member tracking
- [ ] `/server` commands for configuration

**Verify:** Server-level features work as they did in Python Lumina.

---

### 3.4 — Utilities

**Goal:** Remaining minor cogs and cleanup.

**Complexity:** S

**Tasks:**
- [ ] Command sync: slash command registration management
- [ ] Message export: export chat history
- [ ] Any remaining cogs from Python Lumina feature inventory

**Verify:** All v1 cogs ported and functional.

---

## Dependencies

Stages within Phase 3 are largely independent and can be worked in any order. 3.3 may partially overlap with Phase 1B.3 (platform events) if welcome messages are already handled by intents.
