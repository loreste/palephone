# iOS decision gate (Phase 0.8 / M3.3)

## Decision (2026-07-13)

**Gate B — iOS not certified for managed fleet (until CallKit + APNs ship).**

| Platform | Status |
|----------|--------|
| Desktop (macOS / Windows / Linux) | Supported |
| Android | Sideload + video path; physical/background open |
| **iOS** | **Not certified** — packaging docs only (`IOS_SETUP.md`, `ios.yml` preview) |

Procurement language:

> Pale does not currently certify an iOS App Store or MDM-managed fleet. Use
> desktop and Android (sideload) clients. An iOS CallKit/APNs path is tracked
> as Phase 3.4–3.6 in `docs/procurement/IMPLEMENTATION_BACKLOG.md`.

## Gate A requirements (to reverse this decision)

1. Signed TestFlight (or enterprise) build  
2. CallKit incoming call UI  
3. PushKit / APNs VoIP wake  
4. SSO + MFA on device  
5. Device validation checklist signed  

Until then, do **not** market “mobile parity with Teams.”

See also: [android-fleet.md](android-fleet.md), [TEAMS_PARITY.md](../procurement/TEAMS_PARITY.md).
