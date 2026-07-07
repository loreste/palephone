---
name: audit
description: Adversarial audit of recent changes
---

Run an adversarial audit of the codebase:

1. Check for unresolved merge conflict markers in all source files
2. Check for duplicate struct/type definitions in lib.rs and http.rs
3. Check for duplicate migration numbers
4. Spot-check 3-5 features: verify they have migration + server endpoint + frontend UI + proper wiring
5. Check for security issues: stub auth handlers, missing validation, SQL injection vectors
6. Check for dead code: functions defined but never called
7. Verify all `reqwest::Client` calls have timeouts
8. Check that Android-incompatible code is `#[cfg(desktop)]` guarded

Report every real issue found with file paths and line numbers.
