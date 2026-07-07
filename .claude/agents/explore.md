---
name: explore
description: Explore the Pale codebase to answer questions about architecture, features, and code paths
---

Use this agent to explore the Pale softphone codebase. The project is:
- Tauri 2.x desktop + Android app
- React/TypeScript frontend in `src/`
- Rust backend in `src-tauri/`
- PJSIP for SIP calling via `src-tauri/crates/pjsip-sys/`
- Pale server (HTTP API + SIP proxy) in `src-tauri/crates/pale-server/`
- Matrix chat integration in `src-tauri/crates/pale-matrix/`
