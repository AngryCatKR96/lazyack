# lazyack

> Global hotkey to approve AI coding agent prompts — without switching focus.

When Claude Code (or Codex, Aider, ...) asks for `[1] Yes / [2] Yes, allow all / [3] No` and your focus is in another window, you currently have to:

1. Click into the agent terminal
2. Press the digit
3. Click back to where you were

`lazyack` collapses that to a single global hotkey. It auto-detects which agent session is currently waiting for input and routes the keystroke there — no focus change, no manual targeting.

## Status

Pre-alpha. v0.1 work in progress, cmux backend first.

## Why not just `skhd` + `tmux send-keys`?

Three lines of bash will get you "global hotkey → send to a fixed pane." `lazyack` adds the parts that are tedious to script:

- **Auto-routes** to whichever agent surface is *currently waiting for input* (cmux notification state / OSC 9/99/777 / pane-content heuristics).
- **Multi-session safe** — when 2+ agents are waiting, shows a peek HUD before firing so you don't approve into the wrong session.
- **Per-agent profiles** — Claude Code uses 1/2/3, Codex uses y/n, Aider has its own. lazyack detects the agent and applies the right mapping.
- **Dangerous-command guards** — refuses to auto-approve when `rm -rf`, `git push --force`, or `DROP TABLE` patterns are visible in the prompt.

## Roadmap

- [ ] cmux backend (v0.1)
- [ ] tmux backend (v0.2)
- [ ] Multi-session peek HUD
- [ ] Per-agent profile auto-detection
- [ ] Dangerous-command auto-reject heuristics
- [ ] macOS notification action buttons
- [ ] Ghostty / WezTerm backends (community contributions welcome)

## License

MIT
