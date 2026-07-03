# Pale — AI Instructions

## Project
Cross-platform SIP softphone built with Tauri 2.x (Rust backend) + React/TypeScript frontend + PJSIP for media.

## Roadmap
The file `docs/NEXT_STEPS.md` is the single source of truth for all remaining work:
- **Sections 1-3**: Technical backlog (SIP fixes, architecture, UX, contract, QA items)
- **Section 4**: Microsoft Teams enterprise parity gaps (the primary feature roadmap)

### Rules for the roadmap
1. **Before implementing a feature**, check `docs/NEXT_STEPS.md` section 4 to see if it matches a gap item.
2. **After completing a feature**, update the checkbox in `docs/NEXT_STEPS.md` from `[ ]` to `[x]` with the date and commit hash. Example:
   ```
   - [x] Feature name — done 2026-07-03 (abc1234)
   ```
3. **Do not remove completed items** — keep them checked off so progress is visible.
4. If work partially addresses an item, add a note but leave it unchecked until fully done.

## Git commits
- **Never mention Claude, AI, or add Co-Authored-By lines** in commit messages. Commits should read as normal developer commits.

## Code conventions
- Rust server code lives in `src-tauri/crates/pale-server/`
- Client SIP engine in `src-tauri/crates/pale-core/`
- Matrix integration in `src-tauri/crates/pale-matrix/`
- PJSIP FFI bindings in `src-tauri/crates/pjsip-sys/`
- React frontend in `src/`
- SQL migrations in `src-tauri/crates/pale-server/migrations/`
- Tests: Rust `cargo test`, frontend `npx vitest run`, TypeScript `npx tsc --noEmit`
