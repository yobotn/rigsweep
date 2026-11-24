<div align="center">

<img src="docs/icon.png" width="80" alt="" />

# RigSweep

**Room for your agents.** Free up RAM, CPU and disk on Windows dev machines cluttered by AI-agent sessions.

**[🌐 Homepage](https://rigsweep.com)** · **[⬇️ Download](https://github.com/yobotn/rigsweep/releases/latest)** · **[📖 Antivirus FAQ](#antivirus-false-positives)**

🇬🇧 English · [🇩🇪 Deutsch](README.de.md) · [🇪🇸 Español](README.es.md) · [🇫🇷 Français](README.fr.md) · [🇮🇹 Italiano](README.it.md) · [🇧🇷 Português](README.pt-BR.md) · [🇷🇺 Русский](README.ru.md) · [🇯🇵 日本語](README.ja.md) · [🇰🇷 한국어](README.ko.md) · [🇨🇳 简体中文](README.zh-CN.md) · [🇸🇦 العربية](README.ar.md)

![Windows 10/11](https://img.shields.io/badge/Windows-10%20%7C%2011-0078D4?logo=windows)
![Rust + Tauri 2](https://img.shields.io/badge/Rust-Tauri%202-orange?logo=rust)
![License: MIT](https://img.shields.io/badge/License-MIT-green)
![Price: free](https://img.shields.io/badge/price-free-brightgreen)

<img src="docs/screenshots/cleanup.png" alt="RigSweep Dev Cleanup" width="860">

</div>

---

## Why I built this

<!-- Generated from docs/locales (story.*, stat.*) by scripts/prerender-site.js. Edit it there. -->

In the AI era a dev laptop doesn't run one thing anymore. A normal day means several Claude Code sessions in different terminals, a dev server per project, build watchers, and a browser full of docs and previews.

Every session spawns more processes, and many keep running after you close the terminal. Task Manager only shows a wall of node.exe, python.exe and chrome.exe, with no idea which session or project they belong to.

- 🌐 Chrome (37 processes): **6.5 GB**
- ⬢ Node + Claude (14 processes): **2.5 GB**
- 🧩 Windows services running: **~150**
- 💾 Rust target folders (3 projects): **~120 GB**
- 🌿 Git worktrees (1 project): **~30 GB**

*Measured on the author's 32 GB laptop in the middle of a working day.*

Then I looked at my disk. I was building three Rust apps side by side: RigSweep itself, [KeepRun](https://github.com/yobotn/keeprun) (keeps Node, .NET and Python apps running as Windows services) and [Focuside](https://github.com/yobotn/focuside) (a focused IDE for AI coding agents). Each project's `target` folder had grown to about **40 GB**, so **~120 GB** in total, all of it build output that Cargo never cleans up on its own. And one project alone had about **30 GB** of git worktrees: extra checkouts the agents made for parallel tasks, each with its own build output, still there long after the work was merged. The **Dev Disk** tab found all of it in a single scan.

None of it was a virus or junk. It was the normal leftovers of working with AI agents, plus Windows background stuff a dev box doesn't need. Existing “PC booster” apps either do scary irreversible things or know nothing about dev workflows, so I built the tool I wanted.

> **RigSweep shows what's actually using your machine, sweeps what's stale, and keeps every change reversible.**

---

## Features

### Dev Cleanup: stale dev processes in any stack
Finds **dev servers, build daemons and AI-agent CLIs** and groups each one with its **whole process tree**, project folder, age, RAM and listening ports.

| Stack | What it finds |
|---|---|
| ⬢ Node · Bun · Deno | dev servers (Vite, Astro, Next…), watchers, esbuild |
| 🐍 Python | uvicorn, Flask, Django, Jupyter, Streamlit, Celery, venv processes, Python MCP servers |
| ☕ Java | Gradle & Kotlin daemons, Maven, Spring Boot, Quarkus |
| 🟣 .NET | `dotnet watch/run`, Roslyn compiler server, leftover MSBuild nodes |
| 💎 Ruby · 🐘 PHP · 🦀 Rust · 🐹 Go | `rails`/`ruby`, `php -S`/artisan, `cargo watch/run`, `go run` |
| 🤖 Agent CLIs | Claude Code, Codex, Gemini CLI, opencode, Aider, Goose |
| 🔌 MCP servers | servers your agent started (`@modelcontextprotocol/*`, `@playwright/mcp`, `mcp-server-*`, FastMCP…) and left running after it exited, even behind a `cmd /c npx` launcher |
| 🌐 Automation browsers | Playwright / Puppeteer / `--headless` Chrome & Edge with their whole renderer tree. Your own browser is never listed |
| 🐧 WSL 2 / Docker | the Linux VM (`vmmemWSL`), shut down cleanly with `wsl --shutdown`, never pre-selected |

Python and Java also run ordinary desktop apps, so for those only processes with a clear dev signature are listed.

- 🧟 **Orphan**: its terminal was closed but it kept running. Pre-selected, even if it still holds a port (the classic zombie `localhost:3000`).
- 🔌 **MCP server**: tagged on every row that is one, so you can tell an agent's leftover tool server from your own dev server.
- 👯 **Older copy**: an MCP server that's also running as a newer copy (e.g. under your current agent session) is marked, so the leftover one is easy to spot.
- 📈 **Live meter**: RAM and CPU are sampled every 2 seconds while RigSweep runs (window or tray). Right after a cleanup, manual or automatic, it shows **how much RAM was actually recovered** and CPU before → after, measured, not estimated.
- 🌡️ **Temperature tracking**: CPU and GPU temperatures live on the Dashboard with a 5-minute trend and peak, and a warning when the machine stays above 90 °C. GPU comes from the same counter as Task Manager; CPU from the ACPI thermal zone (no driver installed, no admin needed; approximate on many desktops). Click the tile to switch °C / °F.
- 💤 **Idle**: no CPU activity for a long time. Runtimes with no open port get pre-selected. An **idle agent session in an open terminal is never pre-selected**, because it's just waiting for you (and `claude --resume` brings it back anyway).
  How long counts as idle is set in **Settings** (in minutes, default 120; e.g. 5).
- 🧹 **Auto-cleanup** (off by default, in Settings): every minute RigSweep ends stale dev runtimes it has watched sit with no CPU for the whole idle period. Agent CLIs, the WSL/Docker VM and editor-owned processes are never auto-ended, and each ended tree is logged in History.
- 🟠🔴 **Tray indicator**: the tray icon (and the taskbar button while the window is open) gets an orange dot when stale dev processes hold ≥ 1 GB and a red dot at ≥ 2 GB. Hover the tray icon to see how much. Thresholds are set in Settings, where the dot can also be turned off.
- ♻️ **Build daemon**: Gradle, Kotlin and Roslyn servers are meant to linger. Ending them is harmless; they restart on the next build.
- 🔒 **Locked**: processes owned by **VS Code, Cursor, JetBrains, Visual Studio…** (your language servers) or by a desktop app are never killable from here.
- 🎯 **One row or many**: tick rows and click **End selected**, or use the **End** button on a single row. Both show the same confirmation with what it will free.
- ✅ Nothing is ended without your confirmation, and PID reuse is checked before every kill.

### Dev Disk: node_modules, caches and worktrees
Old projects keep gigabytes of `node_modules`, `.next`, `target`, `bin/obj` and `.venv` long after you stop working on them, package caches only ever grow, and AI tools and agents leave caches and worktrees behind. RigSweep finds all of it and deletes what you pick.

- 🗂️ **Projects**: build output and dependency folders from Node, Rust, .NET, Java/Gradle/Maven and Python. A folder only counts if its project file sits next to it (`package.json`, `Cargo.toml`, `*.csproj`, `pyvenv.cfg`…), so a random `build` folder is never touched. Only projects nobody has edited for 30 days (adjustable) are pre-selected, and projects a running process is using are locked 🔒.
- 📦 **Package caches**: npm, npx, pnpm, Yarn, Bun, pip, uv, Cargo, Go, NuGet, Gradle, Maven, Playwright, Puppeteer, Hugging Face. uv and Go are cleaned with their own commands (`uv cache prune`, `go clean`); for Playwright and Puppeteer only older browser versions go, the newest stays.
- 🎮 **GPU & machine learning**: NVIDIA, AMD, Intel and DirectX shader caches, the CUDA compute cache, PyTorch hub models and compiled extensions, the Triton kernel cache, and unused conda packages (through `conda clean --all`). Ollama and LM Studio models are shown with their size, never deleted.
- 🤖 **AI tools & editors**: the app caches of Cursor, VS Code, Windsurf, the Claude desktop app, Codex and ChatGPT, skipped while the app is open. Cursor’s state backup and old checkpoints can go too (never pre-selected). Chat history (Cursor’s chat database, Claude Code chats, Codex sessions) is only shown with its size, so you can see where the space went. Once Cursor is uninstalled, its chat database and checkpoints can go to the Recycle Bin.
- 🌿 **Git worktrees**: worktrees that Claude Code, Cursor, Codex or you created, with their branch, repo and how long they’ve been idle. Removed with `git worktree remove`, which refuses one with uncommitted or untracked work; the branch and its commits stay. Files git ignores that would go too (like `.env`) are listed first, and worktrees are never pre-selected. Not sure one is safe? **Check with AI** runs the AI coding CLI you already use (Claude Code, Codex, Gemini CLI or Grok) read-only on it: RigSweep hands it what git knows (changes, ignored files, unpushed commits, stashes), it can only read files, and its SAFE / NOT SAFE verdict shows on the row. No CLI installed? It copies the prompt for your agent instead.
- 🖥️ **Windows leftovers**: your temp files and Windows temp files untouched for 7 days, Windows Update downloads, Delivery Optimization files, crash dumps and error reports, the thumbnail cache and the Recycle Bin (never ticked for you). Windows.old is shown with its size only.
- 🐳 **Docker & WSL virtual disks**: the `.vhdx` of Docker Desktop and of every WSL distro grows and never shrinks on its own. **Compact** shrinks it to what Linux really uses. Docker Desktop is quit and WSL is shut down first, and nothing inside the disk is deleted.
- 🎯 **Auto-select**: right on the Dev Disk tab, choose how long a project must sit untouched (3 to 365 days), which folders get ticked (build output, dependencies, package caches) and a minimum size.
- 🚫 **Never clean**: switch off any cache. The ones that mean a big re-download (Playwright, Puppeteer, Hugging Face, Gradle, Maven, NuGet packages, Go modules) start switched off.
- 🙈 **Exclusions**: AppData, tool folders in your profile (`.cargo`, `.nvm`, `.vscode`…), `.git` and Windows are always skipped. Add your own paths or folder names (`vendor`, `old-*`) in Settings, or click **Exclude** on a project.
- 💽 **Drive meter**: Dev Disk, Browser Cleanup and the Dashboard open with your drive's free space now and after cleaning ("58 GB free → 81 GB free") and one big **Free X GB** button. The Dashboard adds up everything the scans found.
- 🔁 **Clean automatically** (Settings → Dev Disk, off by default): once a week, and optionally when C: drops below a size you choose, RigSweep empties only the caches that rebuild themselves. Never projects, the Recycle Bin or slow downloads.
- ⏸️ **Pause, resume or stop** a long cleanup and follow it on a progress bar. A cleanup cut short by a restart can pick up where it left off.
- ✅ Scans your user profile (Desktop, Documents, Downloads, OneDrive…) by default. Nothing is deleted without your confirmation, and every folder is checked again right before deletion.

### Hardware: one tab instead of four tools
The basics you'd otherwise open CPU-Z, GPU-Z, CrystalDiskInfo and CrystalDiskMark for, read from Windows itself. No driver is installed.

- 💽 **Disk health**: Healthy / Wearing out / Problem for each disk, with life left, temperature, power-on hours and uncorrected errors. A failing disk raises a warning on the dashboard.
- ⚡ **Speed test**: sequential read and write plus 4K random reads, per drive. It writes a 512 MB temporary file and deletes it afterwards.
- 🧮 **CPU, GPU, memory**: model, cores and threads, cache, virtualization (needed by WSL 2 and Docker), video memory and driver, RAM type, speed and free slots.
- 🔋 **System and battery**: motherboard, BIOS, Windows build, uptime, and battery wear on laptops.

<img src="docs/screenshots/hardware.png" alt="Hardware" width="860">

### GPU & AI: what runs on your graphics card and NPU
What each app does on the GPU and NPU, read from the same counters Task Manager uses. Nothing is installed, and nothing is polled while the page is closed.

- 🎮 **Per adapter**: load, dedicated and shared memory in use, and in Pro mode a meter for each busy engine (3D, Compute, Copy, Video…).
- 📋 **Per app**: GPU usage, engine and dedicated video memory of every app using the GPU, plus shared memory in Pro mode.
- 🧠 **NPU**: a neural processing unit shows up next to the GPUs, and each app's NPU usage is listed with its GPU usage.
- 🤖 **AI workload**: local AI runtimes that are running (Ollama, LM Studio, llama.cpp, Jan, LocalAI, KoboldCpp, ComfyUI, vLLM, Python with PyTorch or Transformers…) and the video memory they hold. For Ollama, the loaded models and the VRAM each one takes are listed too.

<img src="docs/screenshots/gpu.png" alt="GPU & AI" width="860">

### Boost Report: real before and after
No fake "your PC is 87% faster" numbers. RigSweep **measures** your machine and compares two snapshots: RAM used/free, average CPU, process count, running services, Chrome RAM, Node/agent RAM.

- 📸 A **baseline** is captured automatically before your first tweak.
- 🔁 A **before/after** pair is captured around every Dev Cleanup.
- 📋 **Copy as Markdown** to share your result in an issue, a post, or this README.

### Chrome tab helper
A desktop app **can't** close Chrome tabs, so RigSweep ships a tiny extension ([`chrome-extension/`](chrome-extension/)) with no network access and only the `tabs` / `storage` / `alarms` permissions:
- 💤 **Sleep idle tabs**: frees their memory, but the tab stays in your tab strip
- 🧹 **Close duplicate tabs**
- 🎯 **Close tabs by pattern**, like `localhost:*` or `*.vercel.app`
- ⏰ **Auto-sleep** every 10 minutes (optional)

### Reversible Windows tweaks
A curated catalog ([`catalog/tweaks.json`](src-tauri/catalog/tweaks.json)) of telemetry, Xbox, search indexer, background apps, Defender exclusions for dev folders, power plan and more, each with a risk level:

| Level | Meaning |
|---|---|
| 🟢 `safe` | No noticeable side effects (pre-selected) |
| 🟡 `moderate` | Small trade-off, explained |
| 🔴 `caution` | Disables a feature; you must confirm |

<img src="docs/screenshots/tweaks.png" alt="Tweaks" width="860">

- 💾 **Every original value is backed up** in SQLite before the first change. Restore one tweak, a category, or everything.
- 🛟 A **System Restore point** is created before the first apply.
- 🔁 A **watchdog** re-applies tweaks that a Windows Update silently reverted, and counts how often it happens.
- 🧭 **Edge stays gone**: uninstall Edge from Settings, then one tweak stops Edge Update and Windows Update from installing it again. The WebView2 runtime other apps need isn't touched.
- 🚫 Never touched: Windows Update, the Defender engine, RPC/DCOM/LSA and ~60 other core services ([`PROTECTED_SERVICES`](src-tauri/src/catalog.rs)).

### Process analyzer
CPU / RAM / disk per process, and **which services live inside each `svchost.exe`**. Disable a service or end a process right from the list (core items are locked 🔒). Every app is shown with its own icon.

- ❓ **Why?**: click a process to see what it is: CPU, memory, disk, threads, maker, file and command line, which program started it and which programs it started. Well-known Windows processes (Defender, Windows Search, Windows Update, svchost, WMI…) come with a plain explanation of why they get busy.
- 🛡️ **What Defender is scanning**: the Dashboard puts **See why** next to a process that uses a lot of CPU, and next to Microsoft Defender sooner. For Defender, **Record what it scans** watches it for 20 seconds and lists the folders, file types and processes that made it scan. When most of it is your dev work, it points you to the Defender exclusions tweak for dev folders.

<img src="docs/screenshots/processes.png" alt="Processes" width="860">

### Startup: what Task Manager doesn't show
Task Manager's Startup apps list skips most of what starts with Windows. RigSweep lists all of it, minus the parts of Windows itself:

- 🧩 **Auto-start services** from apps and drivers (VPN clients, OEM utilities, updaters). They're switched to Manual, so they no longer start at boot but the app can still start them when you open it.
- ⏰ **Scheduled tasks** that run at sign-in, at boot or on a schedule.
- 🚀 **Startup apps** from the registry and the Startup folder, turned off with the same switch Task Manager uses.
- 💾 Every change is backed up and can be restored, and the watchdog turns an item off again if an app update switches it back on.

<img src="docs/screenshots/startup.png" alt="Startup" width="860">

### Console pop-ups: what was that window?
A Command Prompt or PowerShell window pops up, closes a moment later, and you never find out what ran. **Speed up → 🪟 Console pop-ups** records every one of them:

- 🪟 Every console window that opens and closes on its own within a minute is listed with the **exact command it ran**, even if it was open for only a few milliseconds.
- 🔎 **What started it**: the scheduled task (by name), the service, the app or Explorer, plus the full parent chain under Details.
- 📜 **What ran inside it**: the programs it started (e.g. a `.bat` file calling `curl` and `reg`) and its exit code.
- 🔁 The same command from the same source is one row with a count (×12), so a task that fires every hour is easy to spot.
- ⏹ **Turn off…** disables the third-party scheduled task behind it, backed up and undoable from Startup or Tweaks like any other startup change.
- 🙈 Commands that run hidden aren't listed, only windows you could actually see. RigSweep reads process starts from Windows' own kernel event stream (ETW) and changes nothing by watching. Switch it off in Settings → Appearance & features.

### Network: which app holds which port
- 📶 **Per adapter**: download and upload speed, link speed and traffic since start-up. Windows only gives per-app byte counts to tools that trace as administrator, so traffic is shown per adapter, not per app.
- 🔌 **Per app**: listening ports (TCP and UDP), the number of open connections and the remote addresses each app talks to.
- 🔍 **Who is using it?**: enter a port to see which process holds it, with its command line, or a file or folder to see which apps have it open or are working inside it. It only looks: nothing is ended from here.

<img src="docs/screenshots/network.png" alt="Network" width="860">

### Find: leftovers and files worth a second look
Where your space went, and three lists to review. Nothing is removed unless you pick it.

- 📊 **What's taking space**: the biggest folders on any drive, one level at a time. Click one to go deeper; *Show* opens it in Explorer. It only reads.
- 🚚 **Move caches off the system drive**: npm, pip, uv, Yarn, Bun, NuGet, Go, Gradle, Playwright, Hugging Face and PyTorch caches move to another drive or a Dev Drive in one click. RigSweep sets the tool's own environment variable for your account.
- 📦 **Programs without an uninstaller**: entries in Installed apps whose uninstaller is gone, and folders in Program Files that no installed program, service, shortcut or task uses. Each entry is saved to a backup file before it's removed, and folders go to the Recycle Bin.
- 🕒 **Old files**: files over 50 MB in Downloads, Desktop and Documents that haven't changed for a year, and installers in Downloads older than 90 days. They go to the Recycle Bin, and a file synced with OneDrive is flagged, because deleting it removes it on every device.
- 🛡️ **Worth a look**: unsigned programs that start with Windows or run from AppData or Temp, names that hide their real type (`invoice.pdf.exe`, a hidden right-to-left character) and unsigned downloads. Being listed isn't a verdict: **Scan with Defender** asks Microsoft Defender about that one file.

<img src="docs/screenshots/finders.png" alt="Find" width="860">

### Simple or Pro
In **🛠️ Settings → Appearance & features**:

- 🧭 **Mode**: **Simple** shows the Dashboard, the cleanups and plain words, and hides the expert pages (Processes, Network). **Pro** shows everything, including GPU engines, connections and the raw numbers. New installs start in Simple.
- 📡 **Monitors**: turn off GPU and VRAM per process, AI workload, Network, Temperatures or Disk health alerts. A monitor that's off is never sampled, so it costs no CPU.

### For AI agents and scripts
- ⌨️ **Headless commands** that need **no admin rights**: `rigsweep --disk-report --json`, `rigsweep --disk-clean --safe [--dry-run] --json` and `rigsweep --port 3000 --json`.
- 🔌 **MCP server**: `rigsweep --mcp` gives Claude Code, Cursor or Codex the tools `drive_space`, `disk_report`, `free_disk_space` (a dry run unless told otherwise) and `port_owner`, plus `report_cleanup`: after it removed folders RigSweep asked it to check, the agent names the paths and RigSweep verifies they are gone and counts the size it measured. For Claude Code: `claude mcp add rigsweep -- "C:\Program Files\RigSweep\rigsweep.exe" --mcp`. Claude Desktop: install `rigsweep-<version>.mcpb` from the [latest release](https://github.com/yobotn/rigsweep/releases/latest). It's also in the [MCP Registry](https://registry.modelcontextprotocol.io/) as `io.github.yobotn/rigsweep`.
- 📖 Full reference: [rigsweep.com/cli](https://rigsweep.com/cli/).

### Updates, your choice
**🛠️ Settings → 🔄 Updates** has a **Check now** button and an optional *Check for updates automatically* (at startup, then once a day), which is **off by default**. When a new version is out, a button appears in the sidebar with the release notes. Nothing is installed until you click **Install and restart**, and only if the download is signed with RigSweep's key. Your tweaks, backups and settings are kept. See the [privacy policy](#privacy-policy) for exactly what a check does.

### 11 languages
The UI is in **English, Deutsch, Español, Français, Italiano, Português (Brasil), Русский, 日本語, 한국어, 简体中文 and العربية** (switch in the sidebar), and so are the installer and the Chrome extension. Each one is localized with the official Windows terms of that language (Task Manager, Services, Task Scheduler…), not translated word for word. Spotted something awkward? Open an issue or a PR on `ui/locales/<code>.js` (app), `docs/locales/<code>.js` (homepage) or `README.<code>.md`.

---

## Before vs after on your machine

Install, open **📊 Boost Report → 📸 Measure now**, do a Dev Cleanup and apply the safe tweaks, reboot, measure again, then click **📋 Copy as Markdown**. Results are welcome in [Discussions](../../discussions)!

<!-- TODO(maintainer): paste your own "Copy as Markdown" result here once you've run it on your machine. -->

> ℹ️ Service tweaks take full effect after a reboot. Dev Cleanup and Chrome tab sleeping show up immediately.

---

## Install

1. Download `RigSweep_x.y.z_x64-setup.exe` from [**Releases**](../../releases).
2. Run it. It asks for admin rights, because RigSweep manages services and policies.
3. The installer asks whether to allow RigSweep in Microsoft Defender (see below). This is your choice.
4. Later versions can be installed from **🛠️ Settings → 🔄 Updates** (from 0.5.0 on).

Or with [Scoop](https://scoop.sh):

```powershell
scoop bucket add yobotn https://github.com/yobotn/scoop-bucket
scoop install yobotn/rigsweep
```

🌐 **Chrome helper:** in RigSweep go to **🛠️ Settings → 🌐 Chrome tab helper → Open extension folder**, then in Chrome: `chrome://extensions` → **Developer mode** → **Load unpacked** → pick that folder.

## Antivirus false positives

RigSweep is open source but **not code-signed yet**. Tools that change services and Defender settings look a lot like malware to machine-learning heuristics, so **Microsoft Defender may delete the exe by mistake**. This happened during development as `Trojan:Win32/Bearfoos.B!ml`, where `!ml` means a heuristic guess rather than a known-malware signature.

What we do about it:
- ❓ **Installer prompt (opt-in):** "Allow RigSweep in Microsoft Defender?" Yes excludes **only the install folder**, and the uninstaller removes that exclusion again. Silent installs never add one.
- 🛠️ **Settings → 🛡️ Antivirus trust:** see the status, and add or remove the exclusion at any time.
- ✍️ **Code signing is planned** through [SignPath Foundation](https://signpath.org) (free for open-source projects). That's the real fix. See the [code signing policy](#code-signing-policy).

If it already got deleted: **Windows Security → Virus & threat protection → Protection history →** pick the RigSweep entry → **Actions → Restore**. Only do this for a download from this repo's Releases page, or for a build you made yourself. You can also [report the false positive to Microsoft](https://www.microsoft.com/wdsi/filesubmission) yourself.

---

## Build from source

```powershell
winget install Rustlang.Rustup        # + VS 2022 Build Tools (C++ workload)
npm install
npm run build                         # -> src-tauri/target/release/rigsweep.exe + bundle/nsis/*-setup.exe
```
Dev run: `npm run dev` (accept UAC; the app relaunches itself elevated).

> 💡 Defender may quarantine your fresh local builds too. Add your clone folder to RigSweep's *Dev folders* list (Settings) and apply the **Defender exclusions for dev folders** tweak, or exclude `src-tauri\target` manually.

### CLI
| Command | Effect |
|---|---|
| `rigsweep.exe` | GUI + tray |
| `rigsweep.exe --apply` | Headless: verify the profile, re-apply drifted tweaks, exit (used by the startup task) |
| `rigsweep.exe --apply --tray` | Same, then stay in the tray with the watchdog |
| `rigsweep.exe --restore-all` | Revert everything to the backed-up originals |

### Tests
```powershell
npm run check                                       # everything CI runs; add -- --fast to skip the Rust tests
cd src-tauri
cargo test                                          # catalog validity, backups, snapshots, command parsing
cargo test live_status   -- --ignored --nocapture   # read-only: detected status of every tweak
cargo test live_dev_scan -- --ignored --nocapture   # read-only: what Dev Cleanup sees right now
```

### Adding tweaks
Add an entry to [`catalog/tweaks.json`](src-tauri/catalog/tweaks.json). Action types are `service` (start: disabled|manual), `task` (path), `registry` (HKLM|HKCU, dword|string), `defender_exclusion` and `power_plan`. `caution` items must include a `warning`. `cargo test` rejects protected services and malformed entries.

## docker-optimize (separate tool)
Not part of the app. [`docker-optimize/`](docker-optimize/) is a standalone, reversible one-time script that caps Docker Desktop's WSL2 VM (memory, CPU, RAM reclaim, sparse disk) and turns off Docker Desktop extras you don't need. It never deletes containers, images or volumes. See its [README](docker-optimize/README.md).

## Where things live
| What | Where |
|---|---|
| Database (backups, history, snapshots) | `%ProgramData%\RigSweep\rigsweep.db` (+ `.bak.1`–`.bak.5`) |
| Startup task | Task Scheduler → `RigSweep-Reapply` |
| Exported profiles | `Documents\RigSweep-profile-*.json` |

Coming from **aiBoost** (the old name)? Your database is migrated automatically on first start, and the old startup task is replaced.

## Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io/), certificate by [SignPath Foundation](https://signpath.org/).
*(Application in progress: releases up to v0.3.0 are **not signed** yet.)*

- **Builds:** release installers are built from the tagged source by [GitHub Actions](.github/workflows/release.yml) on GitHub-hosted runners. Only those builds will be submitted for signing.
- **Committers and reviewers:** [@yobotn](https://github.com/yobotn)
- **Approvers:** [@yobotn](https://github.com/yobotn)

### Privacy policy
This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it. RigSweep can make two kinds of network request, each only after you choose it. **Update check**: when you click *Check now* in Settings, or turn on *Check for updates automatically* (off by default). It downloads `latest.json` from this repo's GitHub Releases and sends nothing about your PC; an update is installed only after you click *Install*, and only if its signature matches RigSweep's signing key. **Anonymous usage stats**: the first time you open RigSweep, a dialog explains them and asks; nothing is sent until you answer it, and *Settings → Anonymous stats* turns them off at any time, which also deletes what this PC sent. About once an hour, a report holds only what was cleaned since the last one (time, type and amount freed, one entry per type), the most demanding well-known apps (such as Chrome or VS Code), once a day the hardware and driver software from a fixed vendor list ([discovery.json](server/discovery.json): ASUS, Dell, HP, NVIDIA, Logitech…) with whether it runs and how much it uses, RAM size and core count, the RigSweep version and Windows build, and a random install ID. It never holds file paths, folder or project names, process names outside those two lists, user or PC names, or hardware IDs; your IP address is used only to look up the country and is never stored. *Show what's sent* displays the exact report. The Chrome extension asks the same question in its popup and, if you agree, sends only hourly counts (tabs slept and closed, tabs and windows open), never URLs, titles or sites. Everything else RigSweep stores (backups, history, measurements) stays in `%ProgramData%\RigSweep` on your PC. Full policy: [rigsweep.com/privacy](https://rigsweep.com/privacy/).

## Disclaimer

RigSweep changes Windows services, scheduled tasks, registry policies and Microsoft Defender settings, and it can end running processes. Every tweak is backed up and can be restored, and nothing happens without your confirmation, but **you use it at your own risk**. It is provided **"as is", without warranty of any kind** (see the [MIT license](LICENSE)). Review each change before applying it, keep your own backups, and don't use it on work- or school-managed devices without your IT team's approval.

RigSweep is an independent project, **not affiliated with** Microsoft, Google, Anthropic, OpenAI or any other tool it mentions. All trademarks belong to their owners.

## Contributing
Issues and PRs are welcome, especially new tweaks (with a clear risk level), more dev stacks and agent CLIs for Dev Cleanup, and translations.

## License
[MIT](LICENSE): free forever, do what you want, no warranty.
