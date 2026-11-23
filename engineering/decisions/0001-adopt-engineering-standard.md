# 0001: Adopt project-engineering-standard at L2

- **Status:** accepted
- **Date:** 2026-11-23
- **Deciders:** @yobotn

## Context

RigSweep has real users, runs elevated, updates itself and collects anonymous usage counters. It is built by one
maintainer working mostly through AI coding agents, so the rules that keep it safe have to be written down where
every agent session reads them, not only in chat or a private memory.

## Options considered

1. **Keep ad-hoc rules**: no overhead, but each new agent session relearns (or misses) them.
2. **Adopt [project-engineering-standard](https://github.com/yobotn/project-engineering-standard) at L1**: AGENTS.md,
   CI, secret scanning. Too low: RigSweep handles user data and ships an auto-updating, elevated binary.
3. **Adopt it at L2, with written exceptions for what doesn't fit a solo, agent-driven project.**

## Decision

Option 3. Profiles: CLI + API service + Web app, with the UI/UX standard for `ui/` and `chrome-extension/` and
only the tools/MCP rows of the AI application profile. `AGENTS.md` carries the commands and the project's hard
rules; `engineering/exceptions.md` lists what is not followed and what stands in for it.

Two L3 items are taken early because the app runs elevated and auto-updates: build provenance attestations on
every release (now) and signed installers (SignPath, pending).

## Consequences

- `npm run check` is the one command; CI runs the same steps plus `security.yml` (zizmor, cargo audit) and CodeQL.
- Actions are pinned to commit SHAs, Dependabot proposes updates after a 7-day cooldown, CI must pass on PRs.
- Exceptions expire on 2027-05-23 and get re-reviewed then.

## Challenge log

- *"Rulesets that require CI would block direct pushes."* The required-checks ruleset lets the maintainer bypass,
  so it gates Dependabot and contributor PRs; a second ruleset (no force-push, no deletion) has no bypass.
- *"Copy the standard's `.gitattributes` and `.editorconfig`?"* No: they force LF, and this repo keeps CRLF.
