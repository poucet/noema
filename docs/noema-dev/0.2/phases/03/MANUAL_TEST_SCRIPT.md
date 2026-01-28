# Phase 03 Manual Test Script

Interactive verification of Unified Content Model features.

**Instructions**: Work through each section. Mark items with `[x]` as you complete them. If something fails, note the issue and continue - we'll fix errors as we go.

---

## Prerequisites

- [ ] App compiles: `cargo build -p noema`
- [ ] App runs: `noema`
- [ ] Fresh database (optional): delete `~/.noema/noema.db` for clean slate

---

## Test 1: Alternatives & Selection (3.8.1)

**Goal**: Verify regenerate creates alternatives and you can switch between them.

### Steps

1. **Start a conversation**
   - [ ] Open noema
   - [ ] Send a message: "Hello, tell me a short joke"
   - [ ] Wait for response

2. **Regenerate response**
   - [ ] Click the regenerate button on the assistant message
   - [ ] Wait for new response
   - [ ] Verify: UI shows indicator that alternatives exist (e.g., "1/2" or navigation arrows)

3. **View alternatives**
   - [ ] Click to view/expand alternatives panel
   - [ ] Verify: Both responses are visible
   - [ ] Verify: Each shows metadata (timestamp, model if available)

4. **Select alternative**
   - [ ] Select the first (original) response
   - [ ] Verify: It becomes the active response shown in conversation
   - [ ] Select the second (regenerated) response
   - [ ] Verify: Conversation updates to show second response

5. **Persistence check**
   - [ ] Send a follow-up message: "That was funny"
   - [ ] Verify: The LLM responds based on whichever joke was selected
   - [ ] Close and reopen the app
   - [ ] Verify: Your selection persisted

**Result**: [ ] PASS / [ ] FAIL
**Notes**:

---

## Test 2: SQL Data Verification (3.8.2)

**Goal**: Verify database has correct structure.

### Steps

Open SQLite and run these queries:

```bash
sqlite3 ~/.noema/noema.db
```

1. **Views table**
   ```sql
   SELECT id, entity_id, name, is_main FROM views LIMIT 5;
   ```
   - [ ] Views exist with entity_id references
   - [ ] Main view has `is_main = 1`

2. **View selections**
   ```sql
   SELECT view_id, turn_id, span_id FROM view_selections LIMIT 10;
   ```
   - [ ] Selections exist linking views to spans at turns
   - [ ] If you regenerated, multiple spans should exist for same turn

3. **Entities table**
   ```sql
   SELECT id, entity_type, name, slug FROM entities LIMIT 10;
   ```
   - [ ] Entities exist with type 'view'
   - [ ] Names/slugs populated for named conversations

4. **Turns and spans**
   ```sql
   SELECT t.id, t.sequence, s.id as span_id, s.role
   FROM turns t
   JOIN spans s ON s.turn_id = t.id
   ORDER BY t.sequence, s.created_at
   LIMIT 20;
   ```
   - [ ] Turns have sequential numbers
   - [ ] Spans have correct roles (user/assistant)
   - [ ] Regenerated turns have multiple spans

**Result**: [ ] PASS / [ ] FAIL
**Notes**:

---

## Test 3: Fresh Install E2E (3.8.3)

**Goal**: Verify clean install works end-to-end.

### Steps

1. **Clean slate**
   ```bash
   mv ~/.noema ~/.noema.backup
   ```
   - [ ] Backup moved

2. **First run**
   - [ ] Run `noema`
   - [ ] App creates fresh database
   - [ ] No errors on startup

3. **Basic conversation**
   - [ ] Start new conversation
   - [ ] Send message, receive response
   - [ ] Message appears correctly

4. **Regenerate works**
   - [ ] Regenerate a response
   - [ ] Alternatives UI appears
   - [ ] Can switch between them

5. **Restore backup**
   ```bash
   rm -rf ~/.noema && mv ~/.noema.backup ~/.noema
   ```
   - [ ] Restored

**Result**: [ ] PASS / [ ] FAIL
**Notes**:

---

## Test 4: Entity Layer (3.8.4)

**Goal**: Verify views are entities and forks use relations.

### Steps

1. **Create a fork**
   - [ ] Start conversation with 3+ messages
   - [ ] Fork from an earlier turn (not the last one)
   - [ ] Verify: New view created

2. **Check entity created**
   ```sql
   SELECT e.id, e.entity_type, v.is_main, v.forked_from_turn_id
   FROM entities e
   JOIN views v ON v.entity_id = e.id
   ORDER BY e.created_at DESC
   LIMIT 5;
   ```
   - [ ] Forked view has entity
   - [ ] `forked_from_turn_id` is set

3. **Check relation created**
   ```sql
   SELECT from_entity_id, to_entity_id, relation_type
   FROM entity_relations
   WHERE relation_type = 'forked_from';
   ```
   - [ ] Fork relation exists between view entities

4. **Views independent**
   - [ ] Continue conversation in forked view
   - [ ] Switch back to main view
   - [ ] Verify: Main view unchanged, fork has new messages

**Result**: [ ] PASS / [ ] FAIL
**Notes**:

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

**Result**: [ ] PASS / [ ] FAIL / [ ] SKIPPED
**Notes**:

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

**Result**: [ ] PASS / [ ] FAIL / [ ] SKIPPED
**Notes**:

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

**Result**: [ ] PASS / [ ] FAIL / [ ] SKIPPED
**Notes**:

---

## Summary

| Test | Status | Notes |
|------|--------|-------|
| 1. Alternatives & Selection | | |
| 2. SQL Data Verification | | |
| 3. Fresh Install E2E | | |
| 4. Entity Layer | | |
| 5. Subconversations | | |
| 6. Document CRUD | | |
| 7. Cross-References | | |

**Overall Phase 03 Status**: [ ] READY FOR PHASE 04 / [ ] NEEDS FIXES

**Blocking Issues**:


**Non-blocking Issues** (can fix in Phase 04):

