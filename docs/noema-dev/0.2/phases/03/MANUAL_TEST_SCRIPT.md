# Phase 03 Manual Test Script

Interactive verification of Unified Content Model features.

**Instructions**: Work through each section. Mark items with `[x]` as you complete them. If something fails, note the issue and continue - we'll fix errors as we go.

---

## Prerequisites

- [x] App compiles: `cargo build -p noema`
- [x] App runs: `noema`
- [ ] Fresh database (optional): delete `~/.noema/noema.db` for clean slate

---

## Test 1: Alternatives & Selection (3.8.1)

**Goal**: Verify regenerate creates alternatives and you can switch between them.

### Steps

1. **Start a conversation**
   - [x] Open noema
   - [x] Send a message: "Hello, tell me a short joke"
   - [x] Wait for response

2. **Regenerate response**
   - [x] Click the regenerate button on the assistant message
   - [x] Wait for new response
   - [x] Verify: UI shows indicator that alternatives exist (e.g., "1/2" or navigation arrows)

3. **View alternatives**
   - [x] Click to view/expand alternatives panel
   - [x] Verify: Both responses are visible
   - [ ] Verify: Each shows metadata (timestamp, model if available) ⚠️ **ISSUE: No metadata displayed**

4. **Select alternative**
   - [x] Select the first (original) response
   - [x] Verify: It becomes the active response shown in conversation
   - [x] Select the second (regenerated) response
   - [x] Verify: Conversation updates to show second response

5. **Persistence check**
   - [x] Send a follow-up message: "That was funny"
   - [x] Verify: The LLM responds based on whichever joke was selected
   - [x] Close and reopen the app
   - [x] Verify: Your selection persisted

**Result**: [x] PASS / [ ] FAIL
**Notes**: Missing metadata on alternatives (non-blocking)

---

## Test 2: SQL Data Verification (3.8.2)

**Goal**: Verify database has correct structure.

### Steps

Open SQLite and run these queries:

```bash
sqlite3 ~/.local/share/noema/database/noema.db
```

1. **Views table**
   ```sql
   SELECT * FROM views LIMIT 5;
   ```
   - [x] Views exist with IDs and timestamps
   - [ ] ⚠️ **SCHEMA GAP**: No `entity_id`, `name`, `is_main` columns - views not linked to entities yet

2. **View selections**
   ```sql
   SELECT view_id, turn_id, span_id FROM view_selections LIMIT 10;
   ```
   - [x] Selections exist linking views to spans at turns
   - [x] Multiple spans exist for regenerated turn

3. **Entities table**
   ```sql
   SELECT id, entity_type, name, slug FROM entities LIMIT 10;
   ```
   - [x] Entities table exists with proper schema
   - [x] Entity exists with type 'conversation'
   - [ ] ⚠️ name/slug empty on conversation entity

4. **Turns and spans**
   ```sql
   SELECT t.id, t.role, s.id as span_id, s.model_id
   FROM turns t
   JOIN spans s ON s.turn_id = t.id
   ORDER BY t.created_at, s.created_at
   LIMIT 15;
   ```
   - [x] Turns have roles (user/assistant) - ordered by created_at, not sequence
   - [x] Spans linked to turns with model_id
   - [x] Regenerated turns have multiple spans

**Result**: [ ] PASS / [x] FAIL (partial - core works, entity integration missing)
**Notes**: Core data model works. Schema differs from spec: views not linked to entities, turns use created_at not sequence, role on turn not span. Entity layer integration is a blocking gap.

---

## Test 3: Fresh Install E2E (3.8.3)

**Goal**: Verify clean install works end-to-end.

### Steps

1. **Clean slate**
   ```bash
   mv ~/.local/share/noema ~/.local/share/noema.backup
   ```
   - [x] Backup moved

2. **First run**
   - [x] Run `noema`
   - [x] App creates fresh database
   - [x] No errors on startup (after copying config for API keys)

3. **Basic conversation**
   - [x] Start new conversation
   - [x] Send message, receive response
   - [x] Message appears correctly

4. **Regenerate works**
   - [x] Regenerate a response
   - [x] Alternatives UI appears
   - [x] Can switch between them

5. **Restore backup**
   ```bash
   rm -rf ~/.local/share/noema && mv ~/.local/share/noema.backup ~/.local/share/noema
   ```
   - [x] Restored

**Result**: [x] PASS / [ ] FAIL
**Notes**: Fresh install needs config copied for API keys (expected behavior)

---

## Test 4: Entity Layer (3.8.4)

**Goal**: Verify views are entities and forks use relations.

### Steps

1. **Create a fork**
   - [x] Start conversation with 3+ messages
   - [x] Fork from an earlier user message - works
   - [ ] ⚠️ **BUG**: Fork from assistant message drops the assistant message

2. **Check fork recorded**
   ```sql
   SELECT * FROM views ORDER BY created_at DESC LIMIT 5;
   ```
   - [ ] ⚠️ **SKIP**: Views not linked to entities (schema gap from Test 2)
   - [x] `forked_from_view_id` and `forked_at_turn_id` ARE set correctly on forked views

3. **Check relation created**
   ```sql
   SELECT from_entity_id, to_entity_id, relation_type
   FROM entity_relations
   WHERE relation_type = 'forked_from';
   ```
   - [ ] ⚠️ **SKIP**: Entity relations not created (views not entities)

4. **Views independent**
   - [ ] Continue conversation in forked view
   - [ ] Switch back to main view
   - [ ] Verify: Main view unchanged, fork has new messages

**Result**: [ ] PASS / [x] FAIL
**Notes**: Fork from assistant message bug; entity layer not integrated with views

---

## Test 5: Subconversations (3.8.5)

**Goal**: Verify spawned subconversations work.

> Note: This requires MCP agent functionality. Skip if not implemented in UI.

### Steps

1. **Trigger subconversation**
   - [ ] Use a feature that spawns a subagent (if available)
   - [ ] Wait for subagent to complete

2. **Verify linkage**
   ```sql
   SELECT from_entity_id, to_entity_id, relation_type, metadata
   FROM entity_relations
   WHERE relation_type = 'spawned_from';
   ```
   - [ ] Subconversation linked to parent

3. **Result appears**
   - [ ] Subconversation result visible in parent conversation

**Result**: [ ] PASS / [ ] FAIL / [x] SKIPPED
**Notes**: MCP agent functionality not yet in UI

---

## Test 6: Document CRUD (3.8.6)

**Goal**: Verify documents, tabs, and revisions work.

> Note: Skip if document UI not yet implemented.

### Steps

1. **Create document**
   - [ ] Create a new document
   - [ ] Add title and content
   - [ ] Save

2. **Check storage**
   ```sql
   SELECT id, entity_id, title FROM documents LIMIT 5;
   SELECT id, document_id, name, position FROM document_tabs LIMIT 10;
   SELECT id, tab_id, content_id, version FROM revisions LIMIT 10;
   ```
   - [ ] Document created with entity
   - [ ] Tab exists
   - [ ] Revision exists with content reference

3. **Edit and revisions**
   - [ ] Edit the document
   - [ ] Save again
   - [ ] Verify: New revision created (version increments)

4. **Multiple tabs** (if UI supports)
   - [ ] Add a second tab
   - [ ] Verify: Both tabs visible
   - [ ] Each tab has independent content

**Result**: [ ] PASS / [ ] FAIL / [x] SKIPPED
**Notes**: Document UI not yet implemented

---

## Test 7: Cross-References (3.8.7)

**Goal**: Verify references and backlinks work.

> Note: Skip if reference UI not yet implemented.

### Steps

1. **Create reference**
   - [ ] Link one entity to another (e.g., document references conversation)

2. **Check storage**
   ```sql
   SELECT from_entity_type, from_entity_id, to_entity_type, to_entity_id, relation_type
   FROM references
   LIMIT 10;
   ```
   - [ ] Reference stored

3. **Backlinks**
   - [ ] View the referenced entity
   - [ ] Verify: Backlinks panel shows incoming reference

**Result**: [ ] PASS / [ ] FAIL / [x] SKIPPED
**Notes**: Reference UI not yet implemented

---

## Summary

| Test | Status | Notes |
|------|--------|-------|
| 1. Alternatives & Selection | ✅ PASS | Missing metadata on alternatives |
| 2. SQL Data Verification | ⚠️ PARTIAL | Core works, entity layer integration missing |
| 3. Fresh Install E2E | ✅ PASS | Needs config copy for API keys |
| 4. Entity Layer | ❌ FAIL | Fork from assistant drops message; entity integration missing |
| 5. Subconversations | ⏸️ NEEDS UI | Backend done (3.3b), UI integration needed |
| 6. Document CRUD | ⏸️ NEEDS UI | Backend done (3.4), UI needed |
| 7. Cross-References | ⏸️ NEEDS UI | Backend done (3.6), UI needed |

**Overall Phase 03 Status**: [ ] READY FOR PHASE 04 / [x] NEEDS FIXES

**Blocking Issues**:
- [ ] Fork from assistant message may link to previous user turn instead (needs investigation)
- [ ] **Entity layer not integrated with views** - views should BE entities (view.id = entity.id), currently standalone table
- [ ] **Entity relations not used for forks** - forked_from should be an entity_relation, not a column
- [ ] Conversation entity name/slug not populated

**UI Integration Needed** (backend done, UI not wired):
- [ ] **Subconversations** - backend done (3.3b), needs UI integration
- [ ] **Document CRUD** - backend done (3.4), needs UI
- [ ] **Cross-references** - backend done (3.6), needs UI

**Non-blocking Issues** (polish after features work):
- [ ] Alternatives panel doesn't show metadata (timestamp, model)
