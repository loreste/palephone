# Pale — UI/UX Specification

> **Version:** 1.0 — June 2026
> **Design Language:** Minimal, dark-first, glassmorphic
> **Target:** Desktop (1024×768 minimum, optimized for 1280×800+)

---

## Table of Contents

1. [Design Philosophy](#1-design-philosophy)
2. [Design System Foundation](#2-design-system-foundation)
3. [Layout Architecture](#3-layout-architecture)
4. [Core Views & Components](#4-core-views--components)
5. [Interaction Patterns & Animations](#5-interaction-patterns--animations)
6. [Accessibility](#6-accessibility)
7. [Responsive Behavior](#7-responsive-behavior)
8. [Recommended Tooling](#8-recommended-tooling)

---

## 1. Design Philosophy

### 1.1 Principles

| Principle | Description |
|-----------|-------------|
| **Glanceable** | Call state, registration status, and audio levels must be instantly readable. A user glancing at the app during a meeting knows exactly what's happening in < 1 second. |
| **Calm by default, urgent when needed** | The UI is quiet and minimal during idle. Incoming calls and errors demand attention with motion, color, and sound. Everything in between is subdued. |
| **One-hand operation** | Core call actions (answer, hangup, mute, hold) are reachable with a single click. No multi-step modals for frequent operations. |
| **Dark-first** | VoIP apps run all day, often alongside other work. A dark UI reduces eye strain and visual competition with primary work apps. Light mode is available but secondary. |
| **Spatial consistency** | Elements don't jump around. The call controls are always in the same place regardless of call state. The user builds muscle memory. |

### 1.2 Visual References

The design draws from:
- **Linear** — clean dark UI, subtle borders, muted colors with accent pops
- **Arc Browser** — glassmorphic panels, soft depth, elegant transitions
- **Discord** — compact information density, clear status indicators
- **Apple FaceTime (macOS)** — frosted call overlay, floating controls

---

## 2. Design System Foundation

### 2.1 Color System

#### Dark Theme (Primary)

```
Background Layers (darkest → lightest):
┌─────────────────────────────────────────────────────┐
│  bg-base       #09090B   (zinc-950)   App background│
│  bg-surface    #18181B   (zinc-900)   Cards, panels │
│  bg-elevated   #27272A   (zinc-800)   Hover states  │
│  bg-overlay    #3F3F46   (zinc-700)   Dropdowns     │
└─────────────────────────────────────────────────────┘

Borders:
  border-subtle    #27272A  (zinc-800)   Panel dividers
  border-default   #3F3F46  (zinc-700)   Input borders
  border-focus     #6366F1  (indigo-500) Focus rings

Text:
  text-primary     #FAFAFA  (zinc-50)    Headings, active items
  text-secondary   #A1A1AA  (zinc-400)   Labels, descriptions
  text-tertiary    #71717A  (zinc-500)   Placeholders, timestamps
  text-inverse     #09090B  (zinc-950)   Text on colored buttons

Semantic Colors:
  accent           #6366F1  (indigo-500) Primary actions, active states
  accent-hover     #818CF8  (indigo-400) Hover on primary actions
  accent-muted     #6366F120           Accent at 12% opacity — subtle bg
  
  success          #22C55E  (green-500)  Registered, call connected
  success-muted    #22C55E20           Green at 12% — status dot glow
  
  destructive      #EF4444  (red-500)   Hangup, errors, unregistered
  destructive-hover#F87171  (red-400)   Hover on destructive actions
  
  warning          #F59E0B  (amber-500)  Registering, call on hold
  warning-muted    #F59E0B20           Amber at 12% — hold indicator

  info             #3B82F6  (blue-500)   Incoming call, transfer
```

#### Light Theme

```
Background Layers:
  bg-base          #FFFFFF              App background
  bg-surface       #F4F4F5  (zinc-100)  Cards, panels
  bg-elevated      #E4E4E7  (zinc-200)  Hover states
  bg-overlay       #D4D4D8  (zinc-300)  Dropdowns

Borders:
  border-subtle    #E4E4E7  (zinc-200)
  border-default   #D4D4D8  (zinc-300)

Text:
  text-primary     #09090B  (zinc-950)
  text-secondary   #52525B  (zinc-600)
  text-tertiary    #A1A1AA  (zinc-400)

Semantic colors remain the same — they're chosen to work on both backgrounds.
```

#### Glassmorphism Tokens

```
glass-bg         rgba(24, 24, 27, 0.72)    Dark glass fill
glass-bg-light   rgba(255, 255, 255, 0.08) Light glass fill (dark theme)
glass-border     rgba(255, 255, 255, 0.06) Subtle glass edge
glass-blur       backdrop-blur(16px)       Frosted effect
glass-blur-heavy backdrop-blur(24px)       Call overlay
```

### 2.2 Typography

**Font stack:** `"Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif`

Inter is chosen for its excellent legibility at small sizes, tabular number support (critical for dialpad/timers), and wide language coverage.

| Token         | Size   | Weight | Line Height | Tracking   | Usage                     |
|---------------|--------|--------|-------------|------------|---------------------------|
| `display`     | 48px   | 600    | 1.1         | -0.02em    | Active call number        |
| `heading-lg`  | 24px   | 600    | 1.3         | -0.01em    | View titles               |
| `heading-md`  | 18px   | 600    | 1.4         | -0.01em    | Section headers           |
| `heading-sm`  | 14px   | 600    | 1.4         | 0          | Card titles               |
| `body`        | 14px   | 400    | 1.5         | 0          | General text              |
| `body-sm`     | 13px   | 400    | 1.5         | 0          | Secondary descriptions    |
| `caption`     | 12px   | 500    | 1.4         | 0.01em     | Timestamps, badges        |
| `mono`        | 14px   | 500    | 1.5         | 0          | SIP URIs, IPs (JetBrains Mono) |
| `dialpad`     | 32px   | 500    | 1.0         | 0.05em     | Dialpad digits            |
| `timer`       | 20px   | 600    | 1.0         | 0.05em     | Call duration (tabular nums) |

**Number rendering:** Always use `font-variant-numeric: tabular-nums` for timers, durations, and the dialpad input to prevent layout shifts as digits change.

### 2.3 Spacing & Grid

Base unit: **4px**

```
spacing-1    4px     Tight inner padding
spacing-2    8px     Default gap between related elements
spacing-3    12px    Padding inside buttons, inputs
spacing-4    16px    Panel padding, card padding
spacing-5    20px    Section spacing
spacing-6    24px    Major section gaps
spacing-8    32px    View-level padding
spacing-10   40px    Top-level layout margins
```

**Layout grid:** 8px grid for all alignment. Components snap to 8px increments.

### 2.4 Border Radius

```
radius-sm    6px     Buttons, inputs, badges
radius-md    8px     Cards, dropdowns
radius-lg    12px    Panels, modals
radius-xl    16px    Floating overlays
radius-full  9999px  Avatar circles, pills, status dots
```

### 2.5 Elevation / Shadows

Shadows use colored tints rather than pure black for a softer feel:

```
shadow-sm     0 1px 2px rgba(0, 0, 0, 0.3)
              Subtle lift for cards

shadow-md     0 4px 12px rgba(0, 0, 0, 0.4)
              Dropdowns, tooltips

shadow-lg     0 8px 24px rgba(0, 0, 0, 0.5),
              0 0 0 1px rgba(255, 255, 255, 0.04)
              Modals, floating panels

shadow-glow-accent    0 0 20px rgba(99, 102, 241, 0.3)
                      Active/focused accent elements

shadow-glow-success   0 0 16px rgba(34, 197, 94, 0.25)
                      Connected call indicator

shadow-glow-destructive 0 0 16px rgba(239, 68, 68, 0.3)
                        Ringing/urgent indicator
```

### 2.6 Iconography

Use **Lucide Icons** (MIT, consistent 24px grid, 1.5px stroke):
- `Phone`, `PhoneOff`, `PhoneIncoming`, `PhoneOutgoing`, `PhoneForwarded`
- `Mic`, `MicOff`, `Volume2`, `VolumeX`
- `Pause`, `Play`, `UserPlus`, `ArrowRightLeft` (transfer)
- `Settings`, `User`, `History`, `Search`, `ChevronDown`

Icon size tokens:
```
icon-sm    16px    Inline with body text
icon-md    20px    Buttons, list items
icon-lg    24px    Section headers, primary actions
icon-xl    32px    Empty states, call status
```

---

## 3. Layout Architecture

### 3.1 App Shell

The app uses a fixed sidebar + main content layout. Total window size defaults to **380×640px** (compact phone-like mode) with an optional expanded mode at **900×640px** showing contacts/history alongside the dialpad.

```
Compact Mode (380×640):
┌──────────────────────────────────┐
│  Title Bar (drag region)    — ✕  │  28px — custom title bar (frameless window)
├──────────────────────────────────┤
│  Status Bar                      │  36px — account + registration status
├──────────────────────────────────┤
│                                  │
│                                  │
│         Main Content Area        │  Flex — view-dependent content
│                                  │
│         (Dialpad / Active Call   │
│          / Settings / etc.)      │
│                                  │
│                                  │
├──────────────────────────────────┤
│  ☎  📋  ⏱  ⚙                   │  48px — Bottom tab navigation
│  Dial Contacts Recent Settings   │
└──────────────────────────────────┘

Expanded Mode (900×640):
┌────────────────────────┬─────────────────────────────────┐
│  Title Bar             │  Title Bar                — ✕   │
├────────────────────────┼─────────────────────────────────┤
│  Status Bar            │  Status Bar                     │
├────────────────────────┼─────────────────────────────────┤
│                        │                                 │
│    Sidebar (280px)     │       Main Content (flex)       │
│                        │                                 │
│  ┌──────────────────┐  │                                 │
│  │ Search contacts  │  │                                 │
│  ├──────────────────┤  │                                 │
│  │ Contact / Recent │  │                                 │
│  │ list (scrollable)│  │                                 │
│  │                  │  │                                 │
│  │  Alice Smith     │  │                                 │
│  │  Bob Chen ●      │  │                                 │
│  │  ...             │  │                                 │
│  └──────────────────┘  │                                 │
├────────────────────────┼─────────────────────────────────┤
│  ☎  📋  ⏱  ⚙         │  (no tabs — sidebar navigates)  │
└────────────────────────┴─────────────────────────────────┘
```

### 3.2 Custom Title Bar

Frameless window with a custom drag region for a seamless look:

```
┌──────────────────────────────────────────┐
│ ● ● ●    Pale                    — □ ✕  │   macOS: native traffic lights (left)
│ (drag region - entire bar)              │   Windows/Linux: custom minimize/maximize/close (right)
└──────────────────────────────────────────┘
```

- Traffic light buttons are positioned natively on macOS via Tauri's `decorations: false` + `hiddenTitle: true`.
- On Windows/Linux, render custom window control buttons aligned right.
- The title bar doubles as the drag region (`data-tauri-drag-region`).

### 3.3 Bottom Navigation Tabs

Four tabs, icon + label, with active state indicator:

```
┌──────────┬──────────┬──────────┬──────────┐
│   ☎️      │   👤     │   🕐     │   ⚙️      │
│ Dialpad  │ Contacts │ Recent   │ Settings │
│ (active: │          │          │          │
│ accent   │ zinc-500 │ zinc-500 │ zinc-500 │
│ bar top) │          │          │          │
└──────────┴──────────┴──────────┴──────────┘
```

Active tab: `text-accent` + 2px top border in accent color.
Inactive: `text-tertiary`, no border.

---

## 4. Core Views & Components

### 4.1 Status Bar

Always visible at the top. Shows registration state and current account.

```
┌──────────────────────────────────────────────────┐
│  ● user@sip.example.com                   ▾  🔊 │
│  (green dot = registered)          account  vol  │
└──────────────────────────────────────────────────┘

States:
  ● Green  + "Registered"       — Ready for calls
  ● Amber  + "Registering..."   — In progress (pulsing dot)
  ● Red    + "Unregistered"     — Failed / offline
  ● Gray   + "No Account"       — Not configured
```

- Clicking the account dropdown (▾) shows a list of configured accounts to switch between.
- Volume icon opens a quick audio device picker popover.
- The status dot uses a CSS `box-shadow` glow animation when pulsing (registering state).

### 4.2 Dialpad View

The primary view. Clean numeric grid with a SIP URI input field.

```
┌──────────────────────────────────────┐
│                                      │
│     ┌──────────────────────────┐     │
│     │ +1 (555) 867-5309     ✕  │     │   Input field — auto-formats
│     └──────────────────────────┘     │   as user types. ✕ clears.
│                                      │
│     ┌────┐   ┌────┐   ┌────┐        │
│     │ 1  │   │ 2  │   │ 3  │        │   Digit buttons: 64×64px
│     │    │   │ABC │   │DEF │        │   Subtle sub-label (letters)
│     └────┘   └────┘   └────┘        │   Tap: scale(0.95) + ripple
│     ┌────┐   ┌────┐   ┌────┐        │
│     │ 4  │   │ 5  │   │ 6  │        │
│     │GHI │   │JKL │   │MNO │        │
│     └────┘   └────┘   └────┘        │
│     ┌────┐   ┌────┐   ┌────┐        │
│     │ 7  │   │ 8  │   │ 9  │        │
│     │PQRS│   │TUV │   │WXYZ│        │
│     └────┘   └────┘   └────┘        │
│     ┌────┐   ┌────┐   ┌────┐        │
│     │ *  │   │ 0  │   │ #  │        │
│     │    │   │ +  │   │    │        │   Long-press 0 → "+"
│     └────┘   └────┘   └────┘        │
│                                      │
│           ┌──────────┐               │
│           │  ☎ Call  │               │   Call button: 56px tall
│           │ (green)  │               │   bg-success, rounded-full
│           └──────────┘               │   Disabled if input empty
│                                      │
│     Backspace ⌫          SIP URI 🔗  │   Secondary actions row
│                                      │
└──────────────────────────────────────┘
```

**Digit button design:**
- Background: `bg-surface` with `border-subtle` border.
- Hover: `bg-elevated`, slight scale up.
- Active/pressed: `scale(0.95)`, brief ripple from touch point.
- Sub-label (ABC, DEF, etc.): `caption` size, `text-tertiary`.
- DTMF tone plays on press (short beep feedback via PJSIP tone generator).

**Input field:**
- Auto-format: Detect numeric input and apply phone number formatting (E.164 aware).
- If input starts with `sip:` or contains `@`, switch to raw SIP URI mode (no formatting, monospace font).
- Paste support: detect pasted phone numbers and format them.

### 4.3 Active Call View

Replaces the main content area when a call is connected. The call controls overlay uses glassmorphism.

```
┌──────────────────────────────────────┐
│  Status Bar (unchanged)              │
├──────────────────────────────────────┤
│                                      │
│         ┌──────────────┐             │
│         │              │             │
│         │   Avatar /   │             │   96px circle avatar
│         │   Initials   │             │   or initials with gradient bg
│         │              │             │
│         └──────────────┘             │
│                                      │
│         Alice Smith                  │   display / heading-lg
│         +1 (555) 867-5309           │   body-sm, text-secondary
│                                      │
│           02:34                      │   timer font, text-secondary
│           (Connected)                │   caption, text-success
│                                      │
│  ┌────────────────────────────────┐  │
│  │        Glass Control Bar       │  │   Glassmorphic bar
│  │                                │  │   glass-bg + glass-blur
│  │   🔇     ⏸️     ⌨️     ↗️      │  │
│  │  Mute   Hold  Keypad Transfer │  │   Icon buttons, 48px
│  │                                │  │   Active state: accent bg
│  └────────────────────────────────┘  │
│                                      │
│           ┌──────────┐               │
│           │ ☎ End   │               │   Hangup: bg-destructive
│           │  (red)   │               │   Full width, prominent
│           └──────────┘               │
│                                      │
└──────────────────────────────────────┘
```

**Control button states:**

| Button    | Default           | Active                      | Disabled          |
|-----------|-------------------|-----------------------------|-------------------|
| Mute      | `MicOff` icon     | Accent bg + `Mic` icon      | Grayed out        |
| Hold      | `Pause` icon      | Warning bg + `Play` icon    | During transfer   |
| Keypad    | `Grid` icon       | Opens DTMF overlay          | —                 |
| Transfer  | `PhoneForwarded`  | Opens transfer flow         | While on hold     |

**Call duration timer:**
- Uses `font-variant-numeric: tabular-nums` to prevent layout jitter.
- Format: `MM:SS` for calls < 1 hour, `HH:MM:SS` for longer.
- Starts from `00:00` when call connects (200 OK), not when dialing.

**Avatar generation:**
- If contact has a photo → show circular photo.
- Otherwise → generate initials avatar with a deterministic gradient based on the contact name hash.
- Gradient palette (based on name hash):

```
Pair 1: #6366F1 → #8B5CF6   (indigo → violet)
Pair 2: #EC4899 → #F43F5E   (pink → rose)
Pair 3: #14B8A6 → #06B6D4   (teal → cyan)
Pair 4: #F59E0B → #EF4444   (amber → red)
Pair 5: #22C55E → #14B8A6   (green → teal)
```

### 4.4 Incoming Call Overlay

A full-screen overlay that slides up from the bottom with a backdrop blur, demanding immediate attention.

```
┌──────────────────────────────────────┐
│░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│  Backdrop: bg-base at 60% opacity
│░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│  + backdrop-blur(8px)
│░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│
│┌────────────────────────────────────┐│
││         Incoming Call              ││  Glass panel slides up
││                                    ││  from bottom (300ms ease-out)
││         ┌──────────┐               ││
││         │  Avatar  │               ││  Pulsing ring animation
││         │  (ring   │               ││  around avatar
││         │  pulse)  │               ││
││         └──────────┘               ││
││                                    ││
││         Bob Chen                   ││
││         sip:bob@example.com        ││
││                                    ││
││   ┌──────────┐   ┌──────────┐     ││
││   │ ✕ Reject │   │ ☎ Accept│     ││  Two large buttons
││   │  (red)   │   │  (green) │     ││  Reject: bg-destructive
││   └──────────┘   └──────────┘     ││  Accept: bg-success
││                                    ││
│└────────────────────────────────────┘│
└──────────────────────────────────────┘
```

**Ring animation:** Three concentric circles expanding outward from the avatar, fading as they grow. Uses CSS `@keyframes`:

```css
@keyframes ring-pulse {
  0%   { transform: scale(1);   opacity: 0.6; }
  100% { transform: scale(1.8); opacity: 0; }
}
/* Three rings staggered at 0s, 0.5s, 1.0s delays, 1.5s duration, infinite */
```

**Window behavior on incoming call:**
- If the app is minimized → restore and bring to front (Tauri `window.unminimize()` + `window.set_focus()`).
- If on another desktop/workspace → flash the taskbar/dock icon.
- Play ringtone via a separate audio stream (not the call audio stream) so it respects the system notification volume.

### 4.5 Transfer Flow

Transfer uses a two-step inline flow, not a modal, to keep context visible.

```
Step 1 — Enter transfer target:
┌──────────────────────────────────────┐
│  Transfer Call                    ✕  │  Replaces the control bar area
│  ┌──────────────────────────────┐    │
│  │ Enter number or SIP URI...   │    │  Input field with search
│  └──────────────────────────────┘    │
│                                      │
│  Recent contacts:                    │  Quick-pick from recents
│  ┌──────────────────────────────┐    │
│  │ 👤 Alice Smith   ext. 201   │    │
│  │ 👤 Support Queue ext. 300   │    │
│  └──────────────────────────────┘    │
│                                      │
│  ┌─────────────┐ ┌──────────────┐    │
│  │ Blind Xfer  │ │ Attended Xfer│    │  Two transfer mode buttons
│  │ (immediate) │ │ (consult 1st)│    │
│  └─────────────┘ └──────────────┘    │
└──────────────────────────────────────┘

Step 2 (Attended only) — Consultation call active:
┌──────────────────────────────────────┐
│  Consulting with Alice Smith  00:12  │
│  Party A (Bob Chen) is on hold       │  Small banner showing held party
│                                      │
│  ┌──────────────┐ ┌──────────────┐   │
│  │ Complete Xfer│ │ Cancel       │   │  Complete = sends REFER
│  │   (accent)   │ │ (returns to  │   │  Cancel = hangup consult,
│  │              │ │  Party A)    │   │  resume Party A
│  └──────────────┘ └──────────────┘   │
└──────────────────────────────────────┘
```

### 4.6 Settings View

Organized into sections with a vertical tab layout.

```
┌──────────────────────────────────────┐
│  Settings                            │
├──────────────────────────────────────┤
│                                      │
│  ┌─────────┐                         │
│  │ Account │ ← active tab            │
│  │ Audio   │                         │
│  │ Network │                         │
│  │ About   │                         │
│  └─────────┘                         │
│                                      │
│  ── SIP Account ─────────────────    │
│                                      │
│  Display Name    ┌───────────────┐   │
│                  │ John Doe      │   │
│                  └───────────────┘   │
│  SIP URI         ┌───────────────┐   │
│                  │ john@sip.co   │   │
│                  └───────────────┘   │
│  Password        ┌───────────────┐   │
│                  │ ••••••••      │   │
│                  └───────────────┘   │
│  Registrar       ┌───────────────┐   │
│                  │ sip.co        │   │
│                  └───────────────┘   │
│  Transport       ┌───────────────┐   │
│                  │ TLS ▾         │   │
│                  └───────────────┘   │
│                                      │
│          ┌────────┐ ┌────────┐       │
│          │ Cancel │ │  Save  │       │
│          └────────┘ └────────┘       │
└──────────────────────────────────────┘
```

#### Audio Settings Panel

```
── Audio Devices ──────────────────────

Microphone       ┌───────────────────┐
                 │ MacBook Pro Mic ▾ │    Dropdown with all input devices
                 └───────────────────┘
                 ████████░░░░░░░░░░░░░    Live mic level meter
                                          (green → yellow → red)

Speaker          ┌───────────────────┐
                 │ External Speaker ▾│    Dropdown with all output devices
                 └───────────────────┘
                 [▶ Test]                  Plays a short test tone

── Audio Processing ───────────────────

Echo Cancellation    ┌──────┐
                     │ ● ON │             Toggle switch
                     └──────┘
Noise Suppression    ┌──────┐
                     │ ● ON │
                     └──────┘
Auto Gain Control    ┌──────┐
                     │ ○ OFF│
                     └──────┘

── Codec Priority ─────────────────────

  ≡  Opus (48 kHz)                        Drag handle for reordering
  ≡  G.722 (16 kHz)
  ≡  PCMU / G.711μ (8 kHz)
  ≡  PCMA / G.711A (8 kHz)
```

**Mic level meter:** Updated at ~15fps via a Tauri event from the Rust backend that samples the audio capture amplitude. Rendered as a segmented bar:
- Green (0–70%): normal speech
- Yellow (70–90%): loud
- Red (90–100%): clipping

### 4.7 Recent Calls View

```
┌──────────────────────────────────────┐
│  Recent                    🔍        │  Search icon → expand to search bar
├──────────────────────────────────────┤
│                                      │
│  Today                               │  Date group headers
│  ┌──────────────────────────────────┐│
│  │ ↗ Alice Smith         2m 34s    ││  ↗ = outgoing (green arrow)
│  │   +1 555 867 5309    10:24 AM   ││  ↙ = incoming (blue arrow)
│  ├──────────────────────────────────┤│  ↙✕ = missed (red arrow)
│  │ ↙ Unknown            0m 52s    ││
│  │   sip:sales@acme.co  9:15 AM   ││
│  ├──────────────────────────────────┤│
│  │ ↙✕ Bob Chen           —         ││  Missed: text-destructive
│  │   +1 555 123 4567    8:02 AM   ││  duration "—" for missed
│  └──────────────────────────────────┘│
│                                      │
│  Yesterday                           │
│  ┌──────────────────────────────────┐│
│  │ ...                              ││
│  └──────────────────────────────────┘│
└──────────────────────────────────────┘
```

- Swipe right (or hover → icon) to call back.
- Swipe left (or hover → icon) to delete entry.
- Click to expand: shows full call details (duration, codec used, quality metrics if available).

### 4.8 Notification & Toast System

Non-modal notifications for secondary events. Appear top-right, stack downward, auto-dismiss.

```
┌──────────────────────────────────────┐
│                    ┌─────────────────┐│
│                    │ ● Registered    ││  Success toast
│                    │ user@sip.co     ││  Green left border
│                    │           3s ━━ ││  Auto-dismiss progress bar
│                    └─────────────────┘│
│                                      │
│  (main content below)               │
└──────────────────────────────────────┘
```

Toast types:
| Type    | Left Border    | Icon           | Auto-dismiss |
|---------|----------------|----------------|--------------|
| Success | `success`      | `CheckCircle`  | 3 seconds    |
| Error   | `destructive`  | `AlertCircle`  | 8 seconds    |
| Warning | `warning`      | `AlertTriangle`| 5 seconds    |
| Info    | `info`         | `Info`         | 4 seconds    |

---

## 5. Interaction Patterns & Animations

### 5.1 Animation Tokens

All animations use consistent timing and easing:

```
duration-fast      100ms    Micro-interactions (button press, toggle)
duration-normal    200ms    Standard transitions (hover, focus)
duration-moderate  300ms    Panel slides, view transitions
duration-slow      500ms    Overlay reveals, incoming call

ease-out           cubic-bezier(0.16, 1, 0.3, 1)      Most transitions
ease-in-out        cubic-bezier(0.65, 0, 0.35, 1)     Symmetric (dialogs)
ease-spring        cubic-bezier(0.34, 1.56, 0.64, 1)  Bouncy (call button press)
```

### 5.2 Key Animations

| Interaction             | Animation                                                   |
|-------------------------|-------------------------------------------------------------|
| **Dialpad press**       | `scale(0.95)` + `duration-fast` + ripple from press point   |
| **Call button press**   | `scale(0.92)` + `ease-spring` bounce back                   |
| **View transition**     | Crossfade (opacity 0→1) + subtle `translateY(8px → 0)`     |
| **Incoming call**       | Slide up from bottom (`translateY(100% → 0)`) + backdrop fade-in |
| **Call connect**        | Avatar scales `1.0 → 1.05 → 1.0` + success glow pulse     |
| **Hold toggle**         | Control bar tints to `warning-muted` bg with 300ms fade     |
| **Mute toggle**         | Icon morphs (`Mic` ↔ `MicOff`) with 150ms crossfade        |
| **Toast enter**         | Slide in from right (`translateX(100% → 0)`) + fade         |
| **Toast exit**          | Fade out + `translateY(-8px)` over 200ms                    |
| **Registration pulse**  | Status dot `opacity: 0.4 → 1.0 → 0.4` loop, 1.5s period   |
| **Tab switch**          | Active indicator slides horizontally to new tab (shared layout animation) |

### 5.3 Keyboard Shortcuts

| Shortcut            | Action                  |
|---------------------|-------------------------|
| `0-9`, `*`, `#`    | Dialpad input / DTMF    |
| `Enter`             | Make call / Answer       |
| `Escape`            | Hangup / Cancel / Back   |
| `M`                 | Toggle mute (in call)    |
| `H`                 | Toggle hold (in call)    |
| `Ctrl/Cmd + ,`      | Open settings            |
| `Ctrl/Cmd + D`      | Focus dialpad            |
| `Ctrl/Cmd + K`      | Quick search / command palette |
| `Backspace`         | Delete last digit        |

### 5.4 Command Palette

Power-user feature. `Ctrl/Cmd + K` opens a quick-action search:

```
┌──────────────────────────────────────────┐
│ 🔍 Type a command or contact...          │
├──────────────────────────────────────────┤
│   ☎ Call Alice Smith                     │  Fuzzy search across:
│   ☎ Call +1 555 867 5309                │  - Contacts
│   ⚙ Open Audio Settings                 │  - Recent numbers
│   ⚙ Switch to account: work@sip.co     │  - Actions/settings
│   📋 Copy last call details             │
└──────────────────────────────────────────┘
```

---

## 6. Accessibility

### 6.1 Requirements

| Area            | Standard                                                     |
|-----------------|--------------------------------------------------------------|
| Color contrast  | WCAG 2.1 AA minimum (4.5:1 for text, 3:1 for large text)   |
| Focus indicators| Visible 2px focus ring (`border-focus`) on all interactive elements |
| Screen readers  | All controls have `aria-label`. Call state announced via `aria-live="assertive"` |
| Keyboard nav    | Full tab navigation. Dialpad navigable with arrow keys.      |
| Motion          | Respect `prefers-reduced-motion`: disable all animations except opacity fades |
| Font scaling    | UI remains functional at 150% OS font scaling                |

### 6.2 ARIA Roles

```tsx
// Call status — announced to screen readers on change
<div role="status" aria-live="assertive" aria-atomic="true">
  Call with Alice Smith — Connected — 02:34
</div>

// Dialpad grid
<div role="group" aria-label="Dialpad">
  <button aria-label="1">1</button>
  <button aria-label="2, A B C">2</button>
  ...
</div>

// Mute toggle
<button
  role="switch"
  aria-checked={isMuted}
  aria-label={isMuted ? "Unmute microphone" : "Mute microphone"}
>
```

---

## 7. Responsive Behavior

The app supports three size classes:

| Class     | Window Width   | Behavior                                              |
|-----------|----------------|-------------------------------------------------------|
| Compact   | 320–480px      | Single column, bottom tabs, no sidebar                |
| Medium    | 480–900px      | Single column with wider padding                      |
| Expanded  | 900px+         | Sidebar + main content, no bottom tabs (sidebar navigates) |

**Minimum window size:** 320×500px (enforced via Tauri `min_width`/`min_height`).

The user can resize freely. The layout adapts using CSS container queries (not media queries, since we're in a webview not a full browser window):

```css
@container app (min-width: 900px) {
  .layout { grid-template-columns: 280px 1fr; }
  .bottom-nav { display: none; }
  .sidebar { display: flex; }
}
```

---

## 8. Recommended Tooling

| Purpose              | Tool                      | Rationale                                    |
|----------------------|---------------------------|----------------------------------------------|
| **UI primitives**    | Radix UI                  | Headless, accessible, unstyled — full control |
| **Styling**          | Tailwind CSS 4.x          | Utility-first, design token mapping, small CSS output |
| **Component kit**    | shadcn/ui                 | Pre-built Radix + Tailwind components, copy-paste into project |
| **State management** | Zustand                   | Minimal boilerplate, TypeScript-first, works well with Tauri events |
| **Animation**        | Framer Motion             | Declarative, layout animations, gesture support |
| **Icons**            | Lucide React              | Consistent, tree-shakeable, 1000+ icons     |
| **Font**             | Inter (variable)          | Self-hosted via `@fontsource/inter`          |
| **Form handling**    | React Hook Form + Zod     | Validation for settings forms                |
| **Date formatting**  | date-fns                  | Lightweight, tree-shakeable date formatting  |

### 8.1 Tailwind Configuration Sketch

```ts
// tailwind.config.ts
export default {
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        base:        "var(--color-bg-base)",
        surface:     "var(--color-bg-surface)",
        elevated:    "var(--color-bg-elevated)",
        accent: {
          DEFAULT:   "var(--color-accent)",
          hover:     "var(--color-accent-hover)",
          muted:     "var(--color-accent-muted)",
        },
        success: {
          DEFAULT:   "var(--color-success)",
          muted:     "var(--color-success-muted)",
        },
        destructive: {
          DEFAULT:   "var(--color-destructive)",
          hover:     "var(--color-destructive-hover)",
        },
        warning: {
          DEFAULT:   "var(--color-warning)",
          muted:     "var(--color-warning-muted)",
        },
      },
      fontFamily: {
        sans: ["Inter", ...defaultTheme.fontFamily.sans],
        mono: ["JetBrains Mono", ...defaultTheme.fontFamily.mono],
      },
      animation: {
        "ring-pulse": "ring-pulse 1.5s ease-out infinite",
        "status-pulse": "status-pulse 1.5s ease-in-out infinite",
      },
    },
  },
};
```

---

## Appendix: Component Inventory

A complete list of React components to implement:

### Shell
- `AppShell` — root layout, manages compact/expanded mode
- `TitleBar` — custom drag region, window controls
- `StatusBar` — registration status, account switcher, volume
- `BottomNav` — tab navigation (compact mode)
- `Sidebar` — contact list + navigation (expanded mode)

### Dialpad
- `DialpadView` — main dialpad view container
- `DialpadInput` — phone number / SIP URI input with formatting
- `DialpadGrid` — 4×3 digit button grid
- `DialpadButton` — individual digit button with ripple
- `CallButton` — green call initiation button

### Call
- `ActiveCallView` — connected call screen
- `CallerAvatar` — avatar circle with initials/photo + gradient
- `CallTimer` — tabular-nums duration display
- `CallControls` — glassmorphic control bar (mute, hold, keypad, transfer)
- `CallControlButton` — individual control with active/inactive states
- `DtmfOverlay` — in-call dialpad for DTMF
- `IncomingCallOverlay` — full-screen incoming call with accept/reject

### Transfer
- `TransferPanel` — inline transfer flow
- `TransferInput` — target input with contact search
- `ConsultationBanner` — shows held party during attended transfer

### Contacts & History
- `ContactList` — scrollable contact list with search
- `ContactItem` — single contact row
- `RecentCallsList` — grouped call history
- `RecentCallItem` — single history entry with direction indicator

### Settings
- `SettingsView` — tabbed settings container
- `AccountSettings` — SIP account configuration form
- `AudioSettings` — device selection, level meter, processing toggles
- `NetworkSettings` — transport, STUN/TURN, port config
- `CodecPriorityList` — drag-to-reorder codec list
- `MicLevelMeter` — real-time audio level bar

### Shared
- `Toast` / `ToastContainer` — notification system
- `CommandPalette` — `Cmd+K` quick action search
- `Toggle` — on/off switch
- `Select` — dropdown select (Radix-based)
- `Input` — text input with label
- `Button` — polymorphic button with variants (primary, secondary, destructive, ghost)
- `Badge` — status badge (registered, on-hold, etc.)
- `Tooltip` — hover tooltip for icon buttons
