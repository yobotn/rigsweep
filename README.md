<div align="center">

# 🧹 RigSweep

**Room for your agents.** Free up RAM, CPU and disk on Windows dev machines cluttered by AI-agent sessions.

**[🌐 Homepage](https://yobotn.github.io/rigsweep/)** · **[⬇️ Download](https://github.com/yobotn/rigsweep/releases/latest)** · **[📖 Antivirus FAQ](#%EF%B8%8F-antivirus-false-positives)**

🇬🇧 English · [🇪🇸 Español](README.es.md) · [🇫🇷 Français](README.fr.md)

![Windows 10/11](https://img.shields.io/badge/Windows-10%20%7C%2011-0078D4?logo=windows)
![Rust + Tauri 2](https://img.shields.io/badge/Rust-Tauri%202-orange?logo=rust)
![License: MIT](https://img.shields.io/badge/License-MIT-green)
![Price: free](https://img.shields.io/badge/price-free-brightgreen)

<img src="docs/screenshots/cleanup.png" alt="RigSweep Dev Cleanup" width="860">

</div>

---

## 📖 Why I built this

In the AI era, my laptop isn't running *one* thing anymore. On a normal day I have several **Claude Code** sessions in different terminals, a dev server per project, build watchers, and a browser full of docs, previews and dashboards. Every new session spawns more Node processes, and when I close a terminal some of them **just keep running in the background**.

It started to feel *cluttered*: fans spinning, RAM always near the top, and no clear picture of what was actually eating it. Task Manager just showed me a wall of `node.exe` and `chrome.exe` without telling me which session, which project, or whether anything still needed it.

So I measured my own machine (32 GB RAM) in the middle of a working day:

| 🔎 What I found | Count | RAM |
|---|---:|---:|
| 🌐 Chrome processes | 37 | **6.5 GB** |
| ⬢ Node + Claude processes (agent CLIs, dev servers, the desktop app) | 14 | **2.5 GB** |
| 🧩 Windows services running (telemetry, Xbox, search indexer…) | ~150 | — |

None of it was "a virus" or "junk". It was the **normal leftovers of working with AI agents**, plus Windows background stuff a dev box doesn't need. Existing "PC booster" apps either do scary irreversible things or know nothing about dev workflows. So I built the tool I wanted:

> **See what's actually using your machine, sweep what's stale, and keep every change reversible.**

---

## ✨ Features

### 🧹 Dev Cleanup: stale dev processes in any stack
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
- 💤 **Idle**: no CPU activity for a long time. Runtimes with no open port get pre-selected. An **idle agent session in an open terminal is never pre-selected**, because it's just waiting for you (and `claude --resume` brings it back anyway).
  How long counts as idle is set in **Settings** (in minutes, default 120; e.g. 5).
- 🧹 **Auto-cleanup** (off by default, in Settings): every minute RigSweep ends stale dev runtimes it has watched sit with no CPU for the whole idle period. Agent CLIs, the WSL/Docker VM and editor-owned processes are never auto-ended, and each ended tree is logged in History.
- 🟠🔴 **Tray indicator**: the tray icon (and the taskbar button while the window is open) gets an orange dot when stale dev processes hold ≥ 1 GB and a red dot at ≥ 2 GB. Hover the tray icon to see how much. Thresholds are set in Settings, where the dot can also be turned off.
- ♻️ **Build daemon**: Gradle, Kotlin and Roslyn servers are meant to linger. Ending them is harmless; they restart on the next build.
- 🔒 **Locked**: processes owned by **VS Code, Cursor, JetBrains, Visual Studio…** (your language servers) or by a desktop app are never killable from here.
- ✅ Nothing is ended without your confirmation, and PID reuse is checked before every kill.

### 📊 Boost Report: real before and after
No fake "your PC is 87% faster" numbers. RigSweep **measures** your machine and compares two snapshots: RAM used/free, average CPU, process count, running services, Chrome RAM, Node/agent RAM.

- 📸 A **baseline** is captured automatically before your first tweak.
- 🔁 A **before/after** pair is captured around every Dev Cleanup.
- 📋 **Copy as Markdown** to share your result in an issue, a post, or this README.

### 🌐 Chrome tab helper
A desktop app **can't** close Chrome tabs, so RigSweep ships a tiny extension ([`chrome-extension/`](chrome-extension/)) with no network access and only the `tabs` / `storage` / `alarms` permissions:
- 💤 **Sleep idle tabs**: frees their memory, but the tab stays in your tab strip
- 🧹 **Close duplicate tabs**
- 🎯 **Close tabs by pattern**, like `localhost:*` or `*.vercel.app`
- ⏰ **Auto-sleep** every 10 minutes (optional)

### ⚙️ Reversible Windows tweaks
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
- 🚫 Never touched: Windows Update, the Defender engine, RPC/DCOM/LSA and ~60 other core services ([`PROTECTED_SERVICES`](src-tauri/src/catalog.rs)).

### 🔍 Process analyzer
CPU / RAM / disk per process, and **which services live inside each `svchost.exe`**. Disable a service or end a process right from the list (core items are locked 🔒).

### 🌍 3 languages
The UI is in **English, Español and Français** (switch in the sidebar), and so are the installer and the Chrome extension.

---

## 🏁 Before vs after on your machine

Install, open **📊 Boost Report → 📸 Measure now**, do a Dev Cleanup and apply the safe tweaks, reboot, measure again, then click **📋 Copy as Markdown**. Results are welcome in [Discussions](../../discussions)!

<!-- TODO(maintainer): paste your own "Copy as Markdown" result here once you've run it on your machine. -->

> ℹ️ Service tweaks take full effect after a reboot. Dev Cleanup and Chrome tab sleeping show up immediately.

---

## 📦 Install

1. Download `RigSweep_x.y.z_x64-setup.exe` from [**Releases**](../../releases).
2. Run it. It asks for admin rights, because RigSweep manages services and policies.
3. The installer asks whether to allow RigSweep in Microsoft Defender (see below). This is your choice.

🌐 **Chrome helper:** in RigSweep go to **🛠️ Settings → 🌐 Chrome tab helper → Open extension folder**, then in Chrome: `chrome://extensions` → **Developer mode** → **Load unpacked** → pick that folder.

## 🛡️ Antivirus false positives

RigSweep is open source but **not code-signed yet**. Tools that change services and Defender settings look a lot like malware to machine-learning heuristics, so **Microsoft Defender may delete the exe by mistake**. This happened during development as `Trojan:Win32/Bearfoos.B!ml`, where `!ml` means a heuristic guess rather than a known-malware signature.

What we do about it:
- ❓ **Installer prompt (opt-in):** "Allow RigSweep in Microsoft Defender?" Yes excludes **only the install folder**, and the uninstaller removes that exclusion again. Silent installs never add one.
- 🛠️ **Settings → 🛡️ Antivirus trust:** see the status, and add or remove the exclusion at any time.
- ✍️ **Code signing is planned** through [SignPath Foundation](https://signpath.org) (free for open-source projects). That's the real fix. See the [code signing policy](#-code-signing-policy).

If it already got deleted: **Windows Security → Virus & threat protection → Protection history →** pick the RigSweep entry → **Actions → Restore**. Only do this for a download from this repo's Releases page, or for a build you made yourself. You can also [report the false positive to Microsoft](https://www.microsoft.com/wdsi/filesubmission) yourself.

---

## 🛠️ Build from source

```powershell
winget install Rustlang.Rustup        # + VS 2022 Build Tools (C++ workload)
npm install
npm run build                         # -> src-tauri/target/release/rigsweep.exe + bundle/nsis/*-setup.exe
```
Dev run: `npm run dev` (accept UAC; the app relaunches itself elevated).

> 💡 Defender may quarantine your fresh local builds too. Add your clone folder to RigSweep's *Dev folders* list (Settings) and apply the **Defender exclusions for dev folders** tweak, or exclude `src-tauri\target` manually.

### ⌨️ CLI
| Command | Effect |
|---|---|
| `rigsweep.exe` | GUI + tray |
| `rigsweep.exe --apply` | Headless: verify the profile, re-apply drifted tweaks, exit (used by the startup task) |
| `rigsweep.exe --apply --tray` | Same, then stay in the tray with the watchdog |
| `rigsweep.exe --restore-all` | Revert everything to the backed-up originals |

### 🧪 Tests
```powershell
cd src-tauri
cargo test                                          # catalog validity, backups, snapshots, command parsing
cargo test live_status   -- --ignored --nocapture   # read-only: detected status of every tweak
cargo test live_dev_scan -- --ignored --nocapture   # read-only: what Dev Cleanup sees right now
```

### ➕ Adding tweaks
Add an entry to [`catalog/tweaks.json`](src-tauri/catalog/tweaks.json). Action types are `service` (start: disabled|manual), `task` (path), `registry` (HKLM|HKCU, dword|string), `defender_exclusion` and `power_plan`. `caution` items must include a `warning`. `cargo test` rejects protected services and malformed entries.

## 🐳 docker-optimize (separate tool)
Not part of the app. [`docker-optimize/`](docker-optimize/) is a standalone, reversible one-time script that caps Docker Desktop's WSL2 VM (memory, CPU, RAM reclaim, sparse disk) and turns off Docker Desktop extras you don't need. It never deletes containers, images or volumes. See its [README](docker-optimize/README.md).

## 🗂️ Where things live
| What | Where |
|---|---|
| Database (backups, history, snapshots) | `%ProgramData%\RigSweep\rigsweep.db` (+ `.bak.1`–`.bak.5`) |
| Startup task | Task Scheduler → `RigSweep-Reapply` |
| Exported profiles | `Documents\RigSweep-profile-*.json` |

Coming from **aiBoost** (the old name)? Your database is migrated automatically on first start, and the old startup task is replaced.

## 🔏 Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io/), certificate by [SignPath Foundation](https://signpath.org/).
*(Application in progress: releases up to v0.3.0 are **not signed** yet.)*

- **Builds:** release installers are built from the tagged source by [GitHub Actions](.github/workflows/release.yml) on GitHub-hosted runners. Only those builds will be submitted for signing.
- **Committers and reviewers:** [@yobotn](https://github.com/yobotn)
- **Approvers:** [@yobotn](https://github.com/yobotn)

### 🔒 Privacy policy
This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it. RigSweep has no telemetry, no analytics and no update check, and the Chrome extension makes no network requests. Everything it stores (backups, history, measurements) stays in `%ProgramData%\RigSweep` on your PC.

## ⚠️ Disclaimer

RigSweep changes Windows services, scheduled tasks, registry policies and Microsoft Defender settings, and it can end running processes. Every tweak is backed up and can be restored, and nothing happens without your confirmation, but **you use it at your own risk**. It is provided **"as is", without warranty of any kind** (see the [MIT license](LICENSE)). Review each change before applying it, keep your own backups, and don't use it on work- or school-managed devices without your IT team's approval.

RigSweep is an independent project, **not affiliated with** Microsoft, Google, Anthropic, OpenAI or any other tool it mentions. All trademarks belong to their owners.

## 🤝 Contributing
Issues and PRs are welcome, especially new tweaks (with a clear risk level), more dev stacks and agent CLIs for Dev Cleanup, and translations.

## 📄 License
[MIT](LICENSE): free forever, do what you want, no warranty.
