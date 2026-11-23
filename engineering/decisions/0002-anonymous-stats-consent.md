# 0002: Anonymous usage counters with a regional consent default

- **Status:** accepted
- **Date:** 2026-11-23 (records a design from 0.8.x)
- **Deciders:** @yobotn

## Context

The homepage shows live totals (disk and RAM freed, CPU freed at once), and future features are chosen from what
RigSweep actually finds on real machines. That needs some data from installs, without accounts, trackers or
anything that identifies a person or a PC's contents.

## Options considered

1. **No data at all**: safest, but no impact numbers and features guessed blind.
2. **Opt-in everywhere**: very few people opt in, so the numbers mean little.
3. **A regional default**: ask first (opt-in) where the law or the user's region expects it, opt-out elsewhere,
   always with a notice the user sees once and a switch in Settings → Privacy.

## Decision

Option 3. `report::consent_default` (src-tauri/src/report.rs) decides from the Windows home location and time
zone: the `ASK_FIRST` regions and any European time zone get opt-in, others opt-out; anything unknown asks.
The notice is versioned (`NOTICE` in src-tauri/src/share.rs): a new version shows it again to everyone.
What is sent and what never is: [rigsweep.com/privacy](https://rigsweep.com/privacy/).

## Consequences

- The stats server (`server/`) stores counters and keyword-matched vendor names, never paths, file names,
  user names, project names or IP addresses (Cloudflare's country header only).
- Any new field in the report needs a privacy page update and a new `NOTICE` version first.

## Challenge log

- *"Why not just opt-in everywhere?"* Considered; a regional default with a visible notice and a one-click switch
  was judged fair while keeping the numbers meaningful. Revisit if a regulator or user feedback says otherwise.
