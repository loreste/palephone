# Coding Standards

## Rust
- Follow existing patterns in lib.rs, http.rs, pg_store.rs
- Use `ShardedMap` for in-memory state with PostgreSQL write-through
- All HTTP handlers authenticate via `authenticated_principal()` or `authenticated_admin()`
- SSE events broadcast via `broadcast_sse()`
- External HTTP calls must use `reqwest::Client::builder().timeout(Duration::from_secs(30))` — never unbounded
- TCP connections (ClamAV, etc.) must use `tokio::time::timeout`
- Migrations use `IF NOT EXISTS` / `ADD COLUMN IF NOT EXISTS` for idempotency
- Number migrations sequentially — no duplicate numbers

## TypeScript/React
- Follow existing component patterns in `src/components/`
- State management via Zustand stores in `src/store/`
- Server API calls go through `src/lib/tauri.ts` wrappers
- SSE events handled in `src/hooks/useServerEvents.ts`
- Types must match server response shapes (watch for `Option<T>` → `T | null`)

## Testing
- Rust: `cargo test --workspace`
- Frontend: `npx vitest run`
- Type checking: `npx tsc --noEmit`
- All three must pass before committing
