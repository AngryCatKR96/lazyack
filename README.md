# lazyack

> Stop alt-tabbing to Claude Code just to press `1`.

`lazyack` is a tiny macOS daemon that lets you approve AI coding agent prompts
(Claude Code, Codex, Aider, ...) with a **global hotkey** — without ever
leaving the window you're working in. It auto-detects which agent session is
currently waiting for input via [cmux](https://github.com/manaflow-ai/cmux)
and routes your keystroke there.

> **Status: pre-alpha (v0.1).** Single-target routing, cmux-only backend.
> tmux backend and multi-session HUD planned for v0.2.

## Install

### Homebrew

```bash
brew install AngryCatKR96/lazyack/lazyack
```

### From source

```bash
cargo install --git https://github.com/AngryCatKR96/lazyack
```

## Quick start

1. Make sure [cmux](https://github.com/manaflow-ai/cmux) is running.
2. Run `lazyack` in any terminal.
3. While Claude Code shows a `[1] Yes / [2] / [3] No` menu inside cmux, press
   `Ctrl+Opt+Shift+1` (or `2`, `3`) from **any** application.

lazyack routes the keystroke to the right cmux surface and Claude Code accepts
the answer — no focus change.

## How it works

When you press a configured hotkey, lazyack:

1. Receives the global hotkey via macOS Carbon HotKey API.
2. Connects to cmux's Unix socket (`/tmp/cmux.sock`).
3. Calls `notification.list` to find unread waiting agents.
4. Classifies each by the notification body:
   - `"needs your permission"` → numbered menu (1/2/3) — **safe to inject**
   - `"waiting for your input"` → free-text input — **skip** (would corrupt
     user's actual prompt)
   - other → fallback to `surface.read_text` + pane pattern matching
5. Picks a target preferring numbered-menu candidates when the hotkey sends a
   digit.
6. Sends the configured text via `surface.send_text`.
7. Records the notification ID as consumed so the next press doesn't re-fire
   on the same prompt.

## Configuration

`~/.config/lazyack/config.json` (created manually; defaults are used if
absent):

```json
{
  "bindings": [
    { "hotkey": "ctrl+alt+shift+1", "send": "1\n" },
    { "hotkey": "ctrl+alt+shift+2", "send": "2\n" },
    { "hotkey": "ctrl+alt+shift+3", "send": "3\n" }
  ]
}
```

**Hotkey syntax**: `mod+mod+...+key` (case-insensitive).

- Modifier aliases: `cmd`/`command`/`super`/`meta`, `ctrl`/`control`,
  `alt`/`opt`/`option`, `shift`.
- Keys: digits `0`–`9`, letters `a`–`z`, `f1`–`f20`,
  `enter`/`space`/`tab`/`escape`/`backspace`/`delete`/`up`/`down`/`left`/`right`.

**`send` field**: sent as raw text to the target surface. Include `\n` for
Enter. If `send` is a single digit, lazyack only fires when the target shows
a numbered menu (so `1` never lands in a free-text Claude prompt by accident).

### Power-user setup with Karabiner-Elements

Map Caps Lock to Hyper (`Cmd+Ctrl+Opt+Shift`) in Karabiner, then:

```json
{ "hotkey": "cmd+ctrl+alt+shift+1", "send": "1\n" }
```

Now `Caps+1` — two left-hand keys — answers. Hyper has zero conflicts with any
app shortcut.

## `lazyack doctor`

Diagnose configuration, cmux connectivity, and hotkey conflicts before you
trust lazyack with your live agent sessions:

```
$ lazyack doctor
=== lazyack doctor ===

# cmux
[✓] socket reachable: /tmp/cmux.sock
[✓] surface.list -> 4 surfaces
[✓] notification.list -> 9 total, unread: 1 menu / 0 free-text / 0 other

# Hotkeys (3 configured)
[✓] ctrl+alt+shift+1 can be registered
[✓] ctrl+alt+shift+2 can be registered
[✓] ctrl+alt+shift+3 can be registered

# Karabiner-Elements
[!] config detected at /Users/me/.config/karabiner/karabiner.json
    If a hotkey above can be registered but never fires, a Karabiner rule
    may be remapping it. Use Karabiner-EventViewer to verify.

=== summary: 6 ok / 1 warning / 0 error ===
```

Doctor warns about commonly-conflicting combos (browser tab switching, macOS
screenshots, Spotlight, Raycast default hotkey, cmux's own global hotkey) and
detects Karabiner-Elements (which intercepts keys before Carbon HotKey API
and is the most common reason a registered hotkey never fires).

## Limitations

- **macOS only.** Carbon HotKey + NSApplication run loop. No Linux/Windows port
  on the v0.x roadmap.
- **Requires cmux as the backend.** Notification routing depends on cmux's
  socket API. tmux backend planned for v0.2.
- **Sequential routing when multiple agents are waiting.** Each press picks
  the first menu candidate cmux returns; consumed-tracking ensures the next
  press picks the next one. Works well for "same answer to all" but means
  the user can't pick a specific session from a queue. v0.2 introduces a
  threshold-based delegation (skip + ask user to handle in cmux directly)
  for ambiguous cases.
- **Pattern-matching pane detection.** When the notification body doesn't
  clearly indicate menu vs free-text, lazyack falls back to scanning the last
  20 lines of the pane for `1.` / `[1]` / `[2]` patterns. False negatives
  are possible if Claude Code changes its TUI format.

## Changes

See [Releases](https://github.com/AngryCatKR96/lazyack/releases) and the
[git history](https://github.com/AngryCatKR96/lazyack/commits/main). New
features land as the need shows up — no committed roadmap.

### Notable design choices

- **No HUD/GUI.** lazyack stays a tiny daemon. When auto-routing is
  ambiguous (multiple menus competing for the same hotkey), the
  long-term plan is to step back and let cmux's sidebar do the
  disambiguation, not to grow into a GUI app.
- **Pane scan fallback was tried and reverted.** Commits `4132651` /
  `b765c98`. Closing the "menu shown but cmux notification not yet
  fired" timing gap via pane scanning produced too many subtle issues
  (consumed-tracker TTL regression, scan↔notification double-fire race,
  numbered-output false positives, current-workspace blindness). May
  revisit if cmux exposes better notification timing/state APIs.
- **Granular notification dismiss is a cmux limitation.**
  `notification.clear` ignores parameters and wipes everything, so
  lazyack tracks consumed notifications in-process instead of asking
  cmux to mark a single one read.

## Contributing

Issues and PRs welcome. The codebase is small (~700 LOC across `src/cmux.rs`,
`src/config.rs`, `src/daemon.rs`, `src/doctor.rs`). Examples in `examples/`
demonstrate the cmux socket integration:

- `examples/cmux_probe.rs` — discover available JSON-RPC methods
- `examples/cmux_send_test.rs` — verify `surface.send_text` targeting
- `examples/lookup_dryrun.rs` — show what lazyack would route to right now,
  without injecting

```bash
cargo test           # 19 unit tests
cargo run -- doctor  # smoke-test cmux integration
```

## License

MIT
