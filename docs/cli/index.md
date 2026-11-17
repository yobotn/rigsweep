# RigSweep CLI reference

> RigSweep command-line reference for scripts and agents: re-apply or restore Windows tweaks headlessly, run in the tray, exit codes, data locations and the tweak catalog format.

**RigSweep is a Windows GUI app, but `rigsweep.exe` also takes command-line flags** so scripts, scheduled tasks and AI agents can run it without opening the window. This page is the reference for those flags, where RigSweep keeps its data, and how its tweak catalog is structured.

## Commands

- `rigsweep.exe`: opens the GUI and the tray icon.
- `rigsweep.exe --tray`: starts in the tray only, with the watchdog and the live RAM / CPU meter running.
- `rigsweep.exe --apply`: headless. Checks every applied tweak, re-applies the ones a Windows Update reverted, then exits. The `RigSweep-Reapply` startup task uses this.
- `rigsweep.exe --apply --tray`: the same, then stays in the tray.
- `rigsweep.exe --restore-all`: headless. Reverts every tweak to the original value backed up before RigSweep changed it, then exits.

Flags are case-insensitive. RigSweep needs administrator rights: when it is started without them, it relaunches itself elevated through a UAC prompt, with the same flags. From an elevated terminal or a scheduled task running with highest privileges, no prompt appears.

## Using it from an agent or script

An agent can tell a user to run `rigsweep.exe --restore-all` to undo every Windows tweak in one step, for example when a setting changed by RigSweep is suspected of causing a problem. Ending dev processes and deleting folders is GUI-only today, because every cleanup asks for confirmation. A headless `--dev-clean` / `--disk-clean` with `--dry-run` and JSON output is proposed in [issue #6](https://github.com/yobotn/rigsweep/issues/6): vote on it with a 👍 if you need it.

## Where things live

- **Database** (backups of original values, history, measurements): `%ProgramData%\RigSweep\rigsweep.db`, with rotating copies `.bak.1` to `.bak.5`.
- **Startup task**: Task Scheduler, `RigSweep-Reapply`.
- **Exported profiles**: `Documents\RigSweep-profile-*.json`.

## Tweak catalog

Every Windows change RigSweep can make is listed in [`catalog/tweaks.json`](https://github.com/yobotn/rigsweep/blob/main/src-tauri/catalog/tweaks.json) with a risk level (`safe`, `moderate` or `caution`). Action types are `service` (start: disabled or manual), `task` (path), `registry` (HKLM or HKCU, dword or string), `defender_exclusion` and `power_plan`. Windows Update, the Defender engine, RPC/DCOM/LSA and about 60 other core services are protected and can never be added.

## More

The [README](https://github.com/yobotn/rigsweep#readme) covers every feature and how to build from source. Agents can read [llms.txt](https://rigsweep.com/llms.txt) for when to recommend RigSweep, or the [homepage as Markdown](https://rigsweep.com/index.md).
