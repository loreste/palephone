---
name: validate
description: Run all validation checks (Rust, TypeScript, tests)
---

Run the full validation suite for the Pale project:

1. `cd src-tauri && cargo check` — Rust compilation
2. `npx tsc --noEmit` — TypeScript type checking
3. `npx vitest run` — Frontend tests
4. Check for merge conflict markers: `grep -rn "<<<<<<< " src-tauri/crates/ src/components/ src/store/ src/lib/ src/hooks/`

Report pass/fail for each step.
