# Phase 1: Observations

Learnings, patterns, and gotchas discovered during Phase 1.

---

## Codebase Patterns

### Icon System
- Codebase uses **inline SVG icons** (no external library like Lucide)
- Examples in ModelSelector.tsx: TextIcon, VisionIcon, EmbeddingIcon, StarIcon
- New icons added: PrivateIcon (shield), CloudIcon, CopyIcon, CheckIcon, LockIcon

### Type Generation
- Types auto-generate from Rust via ts-rs (`/src/generated/`)
- Run type generation after adding new Rust types with `#[derive(TS)]`

### Provider Classification

**Local providers** (privacy-safe):
- `ollama`, `llama.cpp` / `llamacpp`, `localai`, `lmstudio`

**Cloud providers** (data leaves device):
- `anthropic`, `openai`, `gemini`, `openrouter`, `groq`

---

## Technical Notes

- No toast library installed - used inline feedback (checkmark icon) instead
- Pre-existing TypeScript errors in codebase (19 errors) - not from Phase 1 changes
- Vite build works despite tsc errors
- `ModelCapability` enum is now the single source of truth for model features

---

## Architecture Decisions

1. **Privacy as Capability**: Rather than hardcoding provider lists, privacy is a `ModelCapability::Private` variant. This is extensible and keeps the logic in one place.

2. **ToolConfig Future-Proofing**: The `ToolConfig` type supports granular control (specific servers, specific tools) even though current UI is just on/off. This avoids breaking changes later.
