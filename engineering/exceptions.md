# Exceptions to the engineering standard

RigSweep follows [project-engineering-standard](https://github.com/yobotn/project-engineering-standard) at **L2**
(see `AGENTS.md`). These are the rules it doesn't meet, why, and what stands in for them. Per the standard's
governance they expire and get re-reviewed, not silently renewed. G3 (secrets) and G6 (irreversible actions)
are never excepted.

| Rule | Reason | Risk & mitigation | Owner | Expires |
|---|---|---|---|---|
| G2, GIT-01, GIT-11 (reviewed PRs into `main`) | One maintainer working through AI agents; there is no second reviewer | `main` ruleset blocks force-push and deletion; CI must pass on PRs (Dependabot, contributors); security-relevant changes get a separate `/code-review` or `/security-review` agent session before release | @yobotn | 2027-05-23 |
| G12, AI-23 (agents never approve their own work, independent review at L2) | Same: no second human | A fresh agent session reviews releases without the author session's context; findings are verified before acting | @yobotn | 2027-05-23 |
| GIT-02, LC-01, AI-20 (branch names, an issue per change) | Plans are agreed in chat and land as small direct commits | User-facing features still get an issue (vote hub #12); commit messages say what and why | @yobotn | 2027-05-23 |
| GIT-03, LC-11 (Conventional Commits, generated changelog) | Plain-sentence commits; release notes are written by hand for users | `.release/notes-<version>.md` → GitHub release + in-app updater | @yobotn | 2027-05-23 |
| SC-15 (environment reviewers for production deploys) | Solo | The maintainer confirms every push to `main` (it deploys); `deploy.yml` runs the site checks and server tests before uploading | @yobotn | 2027-05-23 |
| DX-30, DX §5 (formatter on commit / save) | `cargo fmt` would rewrite all of `src-tauri/` (the code isn't rustfmt-formatted) | No formatter step; style kept by review | @yobotn | 2027-05-23 |
| DX-52 (devcontainer) | The app is Windows-only and runs elevated; it can't build in a Linux container | `npm ci` + `npm run check` documented in `AGENTS.md` | @yobotn | 2027-05-23 |
| Standard `.gitattributes` / `.editorconfig` (LF everywhere, trim whitespace) | The working tree is CRLF and `check-i18n.js` compares raw README code blocks | Only `*.sh eol=lf`; no editorconfig line-ending rules | @yobotn | 2027-05-23 |
| AST live pentest (Strix, Shannon) | No staging environment, and AST-02 forbids production | Codebase audit (`/security-review`) before releases; the stats API has integration tests against real Postgres | @yobotn | 2027-05-23 |
| OBS-03, OBS-06, OBS-07, OBS-09 (OpenTelemetry, SLOs, alert runbooks, postmortems) | One small stats service | `/healthz` + `uptime.yml` every few hours; runbook in `server/deploy/README.md` | @yobotn | 2027-05-23 |
| OBS-08 and web-profile analytics (RUM, trackers) | Conflicts with the privacy promise (`docs/privacy/`) | No trackers on the site; only the opt-out anonymous counters described there | @yobotn | 2027-05-23 |
| Web profile: preview deploy per PR | No PRs (see G2) | `npm run check` + local preview of `docs/` before pushing | @yobotn | 2027-05-23 |
| CLI profile: multi-OS tests, XDG config paths | Windows-only by design | Tests run on `windows-latest` | @yobotn | 2027-05-23 |
| SC-20 (trusted publishing) | Nothing is published to a package registry | Installers are built from the tag by `release.yml` with an updater signature; SignPath code signing pending | @yobotn | 2027-05-23 |
