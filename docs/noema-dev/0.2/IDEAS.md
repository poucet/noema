# Ideas

Lightweight inbox for ideas not yet in the roadmap.

## Inbox

| # | Idea | Notes | Added |
|---|------|-------|-------|
| 1 | Access control model for resources | Different people accessing different models, tools, contexts, conversations, documents, assets. Not just is_private but proper ACL/permissions. | 2026-01-10 |
| 4 | Local filesystem as first-class citizen | Easy markdown import/export for AI design conversations. Accelerates agentic behavior by enabling design work with AI models inside noema. | 2026-01-10 |
| 5 | Dynamic Typst functions for data queries | Typst functions that generate content from noema store data (bug counts, stats, charts). Computed on-the-fly in UI, templatized in storage. When injected into LLM context, evaluate and include both raw template code and generated output. | 2026-01-10 |
| 6 | Proactive AI check-ins | AI initiates conversations after inactivity with check-in prompts. Templatizable check-in patterns for different contexts (e.g. project status, mood, goals). | 2026-01-10 |
| 7 | Endless conversation as main entry point | Single continuous conversation that manages context history in the background. No explicit session boundaries. | 2026-01-10 |
| 8 | Auto-journaling from interactions | Log/journal entries automatically appended based on chats and document interactions. Passive capture of activity and insights. | 2026-01-10 |
| 9 | Active Context / Feedback Engine | Shift from "passive storage" to "active shaping". System responds to input with behavioral nudges rather than sitting inert. Core problem: "Dead Text" (notes apps do nothing) vs "Straitjacket" (habit trackers are rigid). Goal: loose coupling between content and process. | 2026-01-10 |
| 10 | Reflexes (lightweight process hooks) | Small rules attached to contexts that fire automatically. Input Reflexes ("type X, ask Y"), Time Reflexes ("no log by 10am, nudge"), Context Reflexes ("enter Deep Work, mute other inputs"). Fast, automatic, trained—like habits. | 2026-01-10 |
| 11 | Soft Schemas / Loose Hierarchy | Hierarchy as View, not Prison. Tag inheritance (#pullups inherits #gym properties). Ad-hoc nesting that can be restructured tomorrow. Contexts are fluid and can overlap. | 2026-01-10 |
| 12 | Neuro nomenclature for Noema | Traces (raw input), Associations (loose hierarchical links), Reflexes (hooks), Signals (system prompts). Aligns with "Noema" (Greek: object of thought). | 2026-01-10 |

## Triaged

Ideas reviewed and assigned a disposition.

| # | Idea | Disposition | Notes |
|---|------|-------------|-------|
| 2 | Editable/recomputable conversation history | roadmap | Captured in UCM Use Case 5 (Edit & Splice), FR-2.6, FR-2.7 |
| 3 | Unified content model | roadmap | See [design/UNIFIED_CONTENT_MODEL.md](design/UNIFIED_CONTENT_MODEL.md) |
