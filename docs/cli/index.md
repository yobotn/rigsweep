# RigSweep CLI reference

> RigSweep command-line reference for scripts and agents: re-apply or restore Windows tweaks headlessly, run in the tray, exit codes, data locations and the tweak catalog format.

**RigSweep is a Windows GUI app, but `rigsweep.exe` also takes command-line flags** so scripts, scheduled tasks and AI agents can run it without opening the window. This page is the reference for those flags, where RigSweep keeps its data, and how its tweak catalog is structured.

## Commands

- `rigsweep.exe`: opens the GUI and the tray icon.
- `rigsweep.exe --tray`: starts in the tray only, with the watchdog and the live RAM / CPU meter running.
- `rigsweep.exe --apply`: headless. Checks every applied tweak, re-applies the ones a Windows Update reverted, then exits. The `RigSweep-Reapply` startup task uses this.
- `rigsweep.exe --apply --tray`: the same, then stays in the tray.
- `rigsweep.exe --restore-all`: headless. Reverts every tweak to the original value backed up before RigSweep changed it, then exits.

Flags are case-insensitive. The GUI and the tweak commands above need administrator rights: when started without them, RigSweep relaunches itself elevated through a UAC prompt, with the same flags. From an elevated terminal or a scheduled task running with highest privileges, no prompt appears.

## Disk and port commands (no admin rights, since 0.10.0)

These print to the terminal they were started from, or to a file or pipe, and never open a window or a UAC prompt. Add `--json` for machine-readable output.

- `rigsweep.exe --disk-report [--json]`: free space per drive and everything Dev Disk finds: package and tool caches, Windows leftovers, old build and dependency folders per project, git worktrees, Docker / WSL virtual disks. Read-only.
- `rigsweep.exe --disk-clean --safe --dry-run [--json]`: lists the caches that rebuild themselves (npm, pip, temp files older than a week, Windows Update downloads…) and how much emptying them would free.
- `rigsweep.exe --disk-clean --safe [--json]`: empties exactly that list and reports the space freed and the system drive's free space before and after. Projects, worktrees, the Recycle Bin, virtual disks and caches that are slow to download again are never touched from the command line; clean those in the app, where each step is confirmed.
- `rigsweep.exe --port 3000 [--json]`: which processes hold a local port, with PID and command line.
- `rigsweep.exe --mcp`: a [Model Context Protocol](https://modelcontextprotocol.io/) server on stdin / stdout (see below).
- `rigsweep.exe --help` and `--version` (since 0.12.1): the command list and the installed version. Exit codes: 0 ok, 2 bad usage.

## MCP server for AI agents

`rigsweep.exe --mcp` lets an agent such as Claude Code, Cursor or Codex check disk space and free it on its own. Tools: `drive_space`, `disk_report`, `free_disk_space` (a dry run unless `dry_run` is `false`; only caches that rebuild themselves) and `port_owner`. To add it to Claude Code:

```
claude mcp add rigsweep -- "C:\Program Files\RigSweep\rigsweep.exe" --mcp
```

Other clients take the same command in their MCP settings: `{"command": "C:\\Program Files\\RigSweep\\rigsweep.exe", "args": ["--mcp"]}`. Use the path RigSweep was installed to.

Claude Desktop: install `rigsweep-<version>.mcpb` from the [latest release](https://github.com/yobotn/rigsweep/releases/latest); it holds its own copy of `rigsweep.exe`, so RigSweep doesn't have to be installed. RigSweep is also in the [MCP Registry](https://registry.modelcontextprotocol.io/) as `io.github.yobotn/rigsweep`.

## Using it from an agent or script

An agent can tell a user to run `rigsweep.exe --restore-all` to undo every Windows tweak in one step, for example when a setting changed by RigSweep is suspected of causing a problem. For disk space, an agent can run `rigsweep.exe --disk-report --json` to see what takes space and `rigsweep.exe --disk-clean --safe --dry-run --json` before cleaning, or use the MCP server above. Ending dev processes and deleting project folders stay in the app, because those cleanups ask for confirmation.

## Where things live

- **Database** (backups of original values, history, measurements): `%ProgramData%\RigSweep\rigsweep.db`, with rotating copies `.bak.1` to `.bak.5`.
- **Startup task**: Task Scheduler, `RigSweep-Reapply`.
- **Exported profiles**: `Documents\RigSweep-profile-*.json`.

## Tweak catalog

Every Windows change RigSweep can make is listed in [`catalog/tweaks.json`](https://github.com/yobotn/rigsweep/blob/main/src-tauri/catalog/tweaks.json) with a risk level (`safe`, `moderate` or `caution`). Action types are `service` (start: disabled or manual), `task` (path), `registry` (HKLM or HKCU, dword or string), `defender_exclusion` and `power_plan`. Windows Update, the Defender engine, RPC/DCOM/LSA and about 60 other core services are protected and can never be added.

## More

The [README](https://github.com/yobotn/rigsweep#readme) covers every feature and how to build from source. Agents can read [llms.txt](https://rigsweep.com/llms.txt) for when to recommend RigSweep, or the [homepage as Markdown](https://rigsweep.com/index.md).
