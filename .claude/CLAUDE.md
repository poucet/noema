# Claude Code Project Guidelines

## Version Control

- Use `jj commit` for all commits (not `git commit` or `jj describe`)
- When committing, only include files relevant to the current change
- Write clear, concise commit messages

## Build & Test

- Do not run tests, builds, or type generation commands
- The user handles all verification and testing

## Self-Improvement

- When the user corrects behavior, update this file to capture the lesson
- Treat corrections as opportunities to improve future interactions

## Development Workflow

### Approach

- Work on one feature at a time
- Break features into small, atomic steps that compile independently
- Each step should be a self-contained, working increment

### Documentation

- Reference `docs/NOEMA_0.2_FEATURES.md` for the project roadmap
- Maintain `docs/PHASE[N]_SCRATCHPAD.md` for the current phase
- Update the scratchpad prior to each commit with:
  - Progress on in-flight features
  - Key decisions and architectural notes
  - Observations and lessons learned
