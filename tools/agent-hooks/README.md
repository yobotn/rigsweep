# Agent hooks

Opt-in notifications and safe-close reporting for coding agents, per
[developer-experience.md §2](../../standards/developer-experience.md#2-notifications--sound).

| File | What |
|---|---|
| `notify.sh` | Cross-platform notifier (macOS, Linux, WSL/Git Bash → Windows, OSC 9 fallback) + bell |
| `notify.ps1` | Windows notifier (BurntToast if installed, else tray balloon) + distinct system sounds |
| `claude-code/settings.example.json` | `Stop` → done, `Notification` → needs input, `SessionEnd` → session status |
| `codex/config.example.toml` | Codex `notify` on turn completion |

## Enable

Everything is off until you opt in:

```bash
export PES_NOTIFY=1          # turn notifications on
export PES_SOUND=0           # optional: no sound
export PES_QUIET_HOURS=22-07 # optional: silent at night
```

PowerShell: `$env:PES_NOTIFY = '1'` (put it in `$PROFILE`).

**Claude Code:** merge `claude-code/settings.example.json` into `.claude/settings.json` (shared) or
`.claude/settings.local.json` (personal). Copy `tools/` into the project or change the paths.

**Codex:** add the `notify` line to `~/.codex/config.toml`.

## Guarantees (DX-13)

- Always exit 0 and return immediately (work happens in the background), so they never block or fail the agent.
- Send nothing over the network; no code or prompt content in notifications beyond the folder name.

> Hook event names and config keys change between agent versions. Check current docs if something doesn't fire.
