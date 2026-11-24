# Threat model (STRIDE-lite)

Scope: the RigSweep app and installer, its update path, the CLI / MCP mode, the Chrome extension, the stats API
(`stats.rigsweep.com`) and the website. Reviewed 2026-11-23; update it with any change to these boundaries.

## Assets

- The user's PC: RigSweep runs **elevated** and can end processes, delete folders, change services, scheduled
  tasks, registry policies and Defender settings.
- The update channel: whoever controls it can run code as administrator on every install.
- The users' privacy: what the stats report and the extension could reveal.
- The site and the counters on it (reputation).

## Trust boundaries and threats

| Boundary | Threat | Mitigation |
|---|---|---|
| Update download → install | **Spoofing / tampering**: a malicious installer served as an update | Tauri updater verifies the minisign signature against the public key compiled into the app; releases only from `release.yml` on a tag, no caches; provenance attestation; HTTPS to GitHub |
| Release pipeline | **Tampering**: a compromised action or dependency | Actions pinned to commit SHAs, `persist-credentials: false`, job-scoped write permissions, zizmor + cargo audit (`security.yml`), CodeQL, Dependabot 7-day cooldown, secret scanning + push protection |
| Release → MCP Registry / MCP bundle | **Spoofing / tampering**: someone lists a fake `io.github.yobotn/rigsweep`, or a bundle that isn't ours | The registry ties `io.github.yobotn/*` to this GitHub account (GitHub OIDC from `release.yml`, no stored token); the listing pins the `.mcpb` URL on this repo's release and its SHA-256; the `.mcpb` is in the provenance attestation; `mcp-publisher` pinned and its sigstore signature verified before it runs |
| App (elevated) ← process and file data | **Elevation / tampering**: crafted process names, paths or command lines that trick a cleanup into ending or deleting the wrong thing | Nothing ends or deletes without confirmation; PID-reuse check before every kill; editor-owned processes locked; deletes limited to known cache / build folders, worktrees removed with `git worktree remove` (no `--force`); tweaks back up first. Data an app keeps for itself (Cursor's chat database and checkpoints) is only shown while that app looks installed (running, its exe in a usual or Scoop place, or an uninstall entry; an unreadable registry counts as installed), checked again right before cleaning, never pre-selected or part of a group Clean, and goes to the Recycle Bin with Windows asking before anything too big for the bin is deleted for good (`FOF_WANTNUKEWARNING`) |
| App → the user's AI agent (copied prompts) | **Tampering / prompt injection**: "Ask your AI" copies names other programs control (uninstall entries, folder and branch names) into a prompt for an agent that has a shell | The user copies and pastes it themselves; the prompt asks for inspection only (no deleting, committing or changing anything) and a SAFE / NOT SAFE answer; RigSweep still does any removal itself, after its own checks and confirmation |
| App (elevated) → the user's AI coding CLI ("Check with AI" on worktrees) | **Elevation / tampering / prompt injection**: the app runs elevated, so the CLI it starts does too; file names and commit messages in the git facts come from the repository and could try to steer the agent | Only an installed `claude`, `codex`, `gemini` or `grok` found on PATH, started without a shell; the backend re-checks every path is a linked worktree and builds the prompt itself (the UI sends paths and a language name only); the agent is limited to reading (Claude Code: `--restricted --safe-mode --strict-mcp-config --tools Read,Glob,Grep --permission-mode dontAsk`; Codex: `--sandbox read-only`; Gemini CLI: headless without `--yolo`; Grok: `--tools read_file,grep,list_dir`, shell tool disallowed) and runs in an empty folder of its own, so no repository's agent settings, hooks or MCP servers load; repository text is fenced and labelled as data; 6-minute timeout; the verdict only labels rows and can tick the ones marked safe, removal still goes through the normal confirmation and `git worktree remove` |
| Agent → MCP `report_cleanup` (space freed with the AI's help) | **Tampering / repudiation**: an agent claims folders it didn't remove, or inflates how much it freed, to pad the Impact numbers | The tool takes paths only, never sizes; a path counts only if RigSweep handed it to an AI itself (a copied prompt or Check with AI, kept locally with the size RigSweep measured, for 30 days) and it no longer exists; each hand-off is credited once; one RigSweep removes itself is dropped first. Credits go to a separate "ai" area, shown as "freed with your AI's help", not added to RigSweep's own disk total and never included in the anonymous stats. The tool deletes nothing; run without admin rights (the usual case) it can't write RigSweep's database and only answers which paths are gone, and the app credits them itself on its next scan |
| CLI / MCP (stdin, agent-controlled) | **Elevation / tampering**: an agent or prompt injection asks for destructive work | Headless mode never runs elevated; `--disk-clean` needs `--safe` and only empties caches that rebuild themselves; `--dry-run`; no process ending or tweaks from the CLI |
| App → stats API | **Information disclosure**: the report revealing paths, names or projects | Only counters and keyword-gated vendor names (ADR 0002); the client IP is used in memory only, for the rate limit and a country lookup, never stored (server/src/web.rs); regional consent default, visible notice, off switch |
| Internet → stats API | **DoS / tampering**: floods or fake reports inflating the public counters | Cloudflare in front (tunnel, no open ports besides SSH); rate limit per IP (`RATE_MAX`, default 30 per 10 minutes); every report is validated with plausibility bounds (server/src/model.rs) and rejected with problem+json errors |
| Internet → admin dashboard | **Spoofing**: guessed or leaked admin password | Basic auth over HTTPS only; secret in `/etc/yobotn/rigsweep-stats.env` (640); rotate per the runbook |
| Chrome extension ← web pages | **Information disclosure**: tab URLs leaving the browser | Permissions: tabs, storage, alarms only; no host permissions; stats send counts, never URLs (`scripts/test-extension-stats.js` asserts it) |
| Server host | **Elevation**: SSH or service compromise | Key-only SSH, no root login, ufw allows 22 only, fail2ban, unattended upgrades; the app runs as `apps` under pm2 |

## Accepted risks

- Unsigned installer until SignPath: users can verify with the provenance attestation and SHA-256 (ADR 0003).
- No live pentest (no staging); see `engineering/exceptions.md`.
