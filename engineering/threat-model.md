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
| App (elevated) ← process and file data | **Elevation / tampering**: crafted process names, paths or command lines that trick a cleanup into ending or deleting the wrong thing | Nothing ends or deletes without confirmation; PID-reuse check before every kill; editor-owned processes locked; deletes limited to known cache / build folders, worktrees removed with `git worktree remove` (no `--force`); tweaks back up first |
| CLI / MCP (stdin, agent-controlled) | **Elevation / tampering**: an agent or prompt injection asks for destructive work | Headless mode never runs elevated; `--disk-clean` needs `--safe` and only empties caches that rebuild themselves; `--dry-run`; no process ending or tweaks from the CLI |
| App → stats API | **Information disclosure**: the report revealing paths, names or projects | Only counters and keyword-gated vendor names (ADR 0002); the client IP is used in memory only, for the rate limit and a country lookup, never stored (server/src/web.rs); regional consent default, visible notice, off switch |
| Internet → stats API | **DoS / tampering**: floods or fake reports inflating the public counters | Cloudflare in front (tunnel, no open ports besides SSH); rate limit per IP (`RATE_MAX`, default 30 per 10 minutes); every report is validated with plausibility bounds (server/src/model.rs) and rejected with problem+json errors |
| Internet → admin dashboard | **Spoofing**: guessed or leaked admin password | Basic auth over HTTPS only; secret in `/etc/yobotn/rigsweep-stats.env` (640); rotate per the runbook |
| Chrome extension ← web pages | **Information disclosure**: tab URLs leaving the browser | Permissions: tabs, storage, alarms only; no host permissions; stats send counts, never URLs (`scripts/test-extension-stats.js` asserts it) |
| Server host | **Elevation**: SSH or service compromise | Key-only SSH, no root login, ufw allows 22 only, fail2ban, unattended upgrades; the app runs as `apps` under pm2 |

## Accepted risks

- Unsigned installer until SignPath: users can verify with the provenance attestation and SHA-256 (ADR 0003).
- No live pentest (no staging); see `engineering/exceptions.md`.
