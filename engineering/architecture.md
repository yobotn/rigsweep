# Architecture

C4-style overview: the system in its context, then its containers. Details live next to the code
(`AGENTS.md` → Structure); the trust boundaries are in [threat-model.md](threat-model.md).

## Context

```mermaid
flowchart LR
  dev(["Developer on Windows"])
  agent(["AI coding agent<br/>(Claude Code, Cursor, Codex…)"])
  rs["RigSweep<br/>app, CLI/MCP, extension"]
  win[("Windows<br/>processes, services, tasks,<br/>registry, Defender, disks")]
  gh["GitHub<br/>releases, latest.json"]
  site["rigsweep.com<br/>site + stats API"]

  dev -- "uses, confirms every change" --> rs
  agent -- "rigsweep --mcp / --disk-report" --> rs
  rs -- "reads, and changes only after confirmation" --> win
  rs -- "update check (opt-in)" --> gh
  rs -- "anonymous counters (consent)" --> site
  dev -- "reads" --> site
```

## Containers

```mermaid
flowchart TB
  subgraph pc["User's PC"]
    ui["ui/<br/>HTML + JS in WebView2"]
    core["src-tauri/<br/>Rust core, runs elevated"]
    cli["rigsweep.exe --mcp / --disk-*<br/>same binary, not elevated"]
    db[("settings, history, backups<br/>SQLite in %ProgramData%\RigSweep")]
    ext["chrome-extension/<br/>tabs, storage, alarms"]
    ui <-- "Tauri commands" --> core
    core --> db
  end
  subgraph cf["Cloudflare (tunnel, no open web ports)"]
    edge["rigsweep.com · www · stats"]
  end
  subgraph vps["yobotn server (DigitalOcean)"]
    srv["server/<br/>rigsweep-stats (Rust, axum), pm2"]
    files["docs/ → /opt/rigsweep/site"]
  end
  pg[("DigitalOcean managed Postgres<br/>database rigsweep")]
  gha["GitHub Actions<br/>ci · security · deploy · release"]

  core -- "report (consent)" --> edge
  ext -- "counts only" --> edge
  edge --> srv
  srv --> files
  srv --> pg
  gha -- "push to main: test, build, rsync, pm2 reload" --> vps
  gha -- "tag v*: installer, .sig, SBOM, provenance" --> rel["GitHub Releases"]
  core -- "update check" --> rel
```
