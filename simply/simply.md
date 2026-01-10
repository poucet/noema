# Simply-Dev

Development workflow system. This is the single source of truth.

## Current State

```yaml
project: "docs/noema-dev"
version: "0.2"
phase: "01"
```

## Directory Structure

```
simply/                       # The system (reusable)
├── simply.md                 # This file
└── templates/
    ├── version/
    │   ├── ROADMAP.md
    │   └── IDEAS.md
    └── phase/
        ├── TASKS.md
        ├── DEVLOG.md
        ├── OBSERVATIONS.md
        ├── SCRATCHPAD.md
        └── CARRYOVER.md

{project}/                    # Project-specific content
└── {version}/
    ├── ROADMAP.md
    ├── IDEAS.md              # Inbox for ideas not yet in roadmap
    └── phases/
        └── {phase}/
            ├── TASKS.md
            ├── DEVLOG.md
            ├── OBSERVATIONS.md
            ├── SCRATCHPAD.md
            └── CARRYOVER.md
```

## Phase Files

| File | Purpose | When to Update |
|------|---------|----------------|
| `TASKS.md` | Task table + feature specs | Mark tasks done, add new tasks |
| `DEVLOG.md` | Chronological changes | After each feature/commit |
| `OBSERVATIONS.md` | Learnings, gotchas | When discovering something notable |
| `SCRATCHPAD.md` | Working notes | Anytime |
| `CARRYOVER.md` | Context for next phase | At end of phase |

## Path Resolution

| Resource | Path |
|----------|------|
| Roadmap | `{project}/{version}/ROADMAP.md` |
| Ideas | `{project}/{version}/IDEAS.md` |
| Phase dir | `{project}/{version}/phases/{phase}/` |
| Version templates | `simply/templates/version/` |
| Phase templates | `simply/templates/phase/` |

---

## Commands

Use `/simply <action>` to manage phases.

### status

Show current phase status.
1. Read this file for current state
2. Read TASKS.md for task counts
3. Report summary

### next-task

Find next task to work on.
1. Read TASKS.md
2. Find next `todo` task (P0 > P1 > P2 > P3)
3. Present details, ask if ready to start

### observe <text>

Add observation to OBSERVATIONS.md.
1. Take text from args or ask
2. Categorize and append
3. Confirm

### idea <text>

Add idea to IDEAS.md inbox.
1. Take text from args or ask
2. Append to Inbox table with date
3. Confirm

### summarize

Generate CARRYOVER.md for phase end.
1. Read TASKS.md, DEVLOG.md, OBSERVATIONS.md
2. Write completed/incomplete features, key context, open questions
3. Save to CARRYOVER.md

### switch <phase>

Switch to different phase.
1. Create phase dir if needed (from templates)
2. Update "Current State" in this file
3. Load new phase context

---

## Pre-Compact

Before compacting, update current phase docs:
- `DEVLOG.md` - Changes made this session
- `OBSERVATIONS.md` - Learnings or gotchas
- `TASKS.md` - Task status updates
- `SCRATCHPAD.md` - Notes to help resume

---

## Starting a New Phase

1. Create `{project}/{version}/phases/{NN}/`
2. Copy templates from `simply/templates/phase/`
3. Copy phase overview from ROADMAP.md to TASKS.md
4. Read previous CARRYOVER.md for context
5. Update "Current State" above

## Phase Transitions

When completing a phase:
1. Update DEVLOG.md with all changes
2. Capture learnings in OBSERVATIONS.md
3. Write CARRYOVER.md for next phase
4. Update TASKS.md with final status
