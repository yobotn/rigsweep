# session-status

Answers *"Can I close this terminal, and is my work safe?"* by reporting the four states from
[developer-experience.md](../../standards/developer-experience.md#1-the-session-lifecycle) separately.

```bash
tools/session-status/session-status.sh                 # human-readable
tools/session-status/session-status.sh --json          # for hooks/status lines
some-long-command; tools/session-status/session-status.sh --exit-code $?
```

```powershell
./tools/session-status/session-status.ps1
./tools/session-status/session-status.ps1 -Json
```

Exit codes: `0` synced · `1` uncommitted / unpushed / merge-or-rebase in progress · `2` not a git repo.

Detects: uncommitted and untracked changes, missing upstream, ahead/behind, stashes, in-progress merge/rebase/cherry-pick.

**Known limit:** a child script can't see its parent shell's background jobs, so it reports them as "not checked"
instead of guessing. Run `jobs` / `Get-Job` yourself, or wire it into your prompt function where those are visible.

## Shell integration (optional)

**bash/zsh: warn on exit if not synced** (`~/.bashrc` / `~/.zshrc`):

```bash
trap '[ -d .git ] && /path/to/session-status.sh || true' EXIT
```

**PowerShell: on exit** (`$PROFILE`):

```powershell
Register-EngineEvent PowerShell.Exiting -Action { if (Test-Path .git) { & /path/to/session-status.ps1 } } | Out-Null
```
