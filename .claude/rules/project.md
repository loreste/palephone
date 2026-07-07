# Pale Project Rules

## Git Commits
- Never mention AI, assistants, or co-authored-by lines in commit messages
- Commits should read as normal developer commits
- Always validate before committing: `cargo check`, `npx tsc --noEmit`, `npx vitest run`

## Code Conventions
- Rust server code: `src-tauri/crates/pale-server/src/`
- Client SIP engine: `src-tauri/crates/pale-core/src/`
- Matrix integration: `src-tauri/crates/pale-matrix/src/`
- PJSIP FFI bindings: `src-tauri/crates/pjsip-sys/`
- React frontend: `src/`
- SQL migrations: `src-tauri/crates/pale-server/migrations/`

## Roadmap
- `docs/NEXT_STEPS.md` is the source of truth for remaining work
- Check it before implementing features to avoid duplication

## Android
- Guard desktop-only code with `#[cfg(desktop)]`
- Provide `#[cfg(mobile)]` stubs for commands that must exist in the invoke handler
- The `keyring` crate is desktop-only; Android uses a graceful fallback
- Test that `cargo check` passes for both desktop and Android targets

## Files to Never Commit
- `.claude/` directory (settings, rules, skills, agents)
- `.mcp.json`
- `.vscode/`
- Internal planning docs (DBA_REVIEW.md, FEATURE_ALIGNMENT_PLAN.md, etc.)
- `.env` files with real secrets (`.env.example` is fine)
