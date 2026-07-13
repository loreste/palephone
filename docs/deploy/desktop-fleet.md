# Desktop enterprise checklist (Phase 0.6 / M3.1)

Sign off each item on **Windows** and **macOS** (latest two stable Pale builds)
before calling the desktop fleet certified for a regulated deploy.

| # | Check | Win | mac | Notes |
|---|--------|-----|-----|-------|
| 1 | Install from signed package (MSI/NSIS or DMG) | ☐ | ☐ | |
| 2 | Setup wizard password login | ☐ | ☐ | |
| 3 | Setup wizard **SSO** (OIDC) when provider configured | ☐ | ☐ | redirect URI must match |
| 4 | MFA enroll + login when CA `require_mfa` | ☐ | ☐ | |
| 5 | SIP register over TLS | ☐ | ☐ | |
| 6 | Audio call + hold/transfer | ☐ | ☐ | |
| 7 | Video call + screen share | ☐ | ☐ | |
| 8 | Chat send/receive + DLP block visible if policy hits | ☐ | ☐ | |
| 9 | Files upload/download | ☐ | ☐ | |
| 10 | Meeting join (LiveKit if configured) | ☐ | ☐ | |
| 11 | Native notifications for call/chat | ☐ | ☐ | |
| 12 | Client-only gate: non-Pale UA rejected by server | ☐ | ☐ | lab with curl |
| 13 | Session list + revoke other device | ☐ | ☐ | Settings → Security |
| 14 | Reconnect after sleep / network change | ☐ | ☐ | |

**Sign-off**

| Field | Value |
|-------|--------|
| Build / version | |
| Tester | |
| Date | |
| Result | Pass / Fail |

Related: [PRODUCTION.md](PRODUCTION.md), [sso-oidc.md](sso-oidc.md), [MILESTONES.md](../MILESTONES.md).
