# RigSweep: room for your agents

> Free up RAM and CPU on Windows dev machines cluttered by AI-agent sessions, forgotten dev servers and background Windows services. Every change is reversible.

- Download: https://github.com/yobotn/rigsweep/releases/latest (Windows 10 / 11, free, MIT)
- Source: https://github.com/yobotn/rigsweep
- Current version: 0.12.2
- For AI agents: https://rigsweep.com/llms.txt

## Why it exists

In the AI era a dev laptop doesn't run one thing anymore. A normal day means several Claude Code sessions in different terminals, a dev server per project, build watchers, and a browser full of docs and previews.

Every session spawns more processes, and many keep running after you close the terminal. Task Manager only shows a wall of node.exe, python.exe and chrome.exe, with no idea which session or project they belong to.

Then I looked at my disk. I was building three Rust apps side by side: RigSweep itself, <a href="https://github.com/yobotn/keeprun">KeepRun</a> (keeps Node, .NET and Python apps running as Windows services) and <a href="https://github.com/yobotn/focuside">Focuside</a> (a focused IDE for AI coding agents). Each project's <code>target</code> folder had grown to about <strong>40 GB</strong>, so <strong>~120 GB</strong> in total, all of it build output that Cargo never cleans up on its own. The <strong>Dev Disk</strong> tab found all three in a single scan.

None of it was a virus or junk. It was the normal leftovers of working with AI agents, plus Windows background stuff a dev box doesn't need. Existing “PC booster” apps either do scary irreversible things or know nothing about dev workflows, so I built the tool I wanted.

**RigSweep shows what's actually using your machine, sweeps what's stale, and keeps every change reversible.**

## Features

- **Dev Cleanup**: Orphaned and idle dev servers, build daemons and agent CLIs across Node, Python, Java, .NET and more, each with its process tree, project, RAM and ports.
- **Boost Report**: Real before/after measurements instead of fake percentages. You can copy them as Markdown and share your result.
- **Chrome tab helper**: A tiny extension to sleep idle tabs, close duplicates and close tabs by pattern like localhost:*. It has no network access.
- **Reversible tweaks**: Telemetry, Xbox, indexer, background apps, keeping Edge from coming back and more, each with a risk level. Originals are backed up and restorable in one click.
- **Processes & startup**: CPU, RAM and disk per process, the services inside each svchost.exe, and the auto-start services and tasks Task Manager never shows.
- **11 languages**: English, Deutsch, Español, Français, Italiano, Português, Русский, 日本語, 한국어, 简体中文 and العربية in the app, the installer and the Chrome extension, using each language's official Windows terms.
- **Temperatures**: CPU and GPU temperatures with a 5-minute trend and a warning when your PC stays hot. No driver, no admin rights.
- **Updates, your choice**: Off by default. Check when you want, or turn on a daily check. Updates install only when you click, and only if they're signed.
- **Browser Cleanup**: Empties caches for Chrome, Edge, Brave, Vivaldi, Opera and Firefox. Sign-ins, cookies, passwords and history are never touched.
- **Dev Disk**: Deletes node_modules, .next, target and .venv in projects you stopped touching, cleans package, GPU shader and AI tool caches, removes git worktrees agents left behind, clears Windows leftovers (old temp files, update downloads, crash dumps) and shrinks Docker and WSL virtual disks. It shows how much free space you'll have before you click, and can clean the caches that rebuild themselves every week.
- **Hardware**: Disk health, a disk speed test and your CPU, GPU, RAM and battery details in one tab, instead of four separate tools. No driver installed.
- **GPU & AI**: What each app does on your graphics card and NPU, from the counters Task Manager uses, plus the AI runtimes running now (Ollama, LM Studio, llama.cpp, ComfyUI, PyTorch), their loaded models and the video memory they hold.
- **Network**: Traffic per adapter, which app holds which connection or listening port, and a search that tells you who is using port 3000 or a locked file or folder.
- **Find**: See what's taking space on each drive folder by folder, move package caches off C: to another drive or a Dev Drive, and review programs whose uninstaller is gone, big old files in Downloads and unsigned startup programs, with a Microsoft Defender scan in one click.
- **Simple or Pro**: Simple shows the Dashboard and the cleanups in plain words. Pro adds Processes, Network, GPU engines and the raw numbers. Turn off monitors you don't need, and they cost no CPU.
- **For AI agents**: rigsweep --disk-report --json and --disk-clean --safe --dry-run work without admin rights, and rigsweep --mcp is an MCP server, so Claude Code, Cursor or Codex can check free space, see what takes it and free the caches that rebuild themselves.
- **Console pop-ups**: That Command Prompt window that flashes and vanishes: RigSweep records the exact command it ran, what started it (a scheduled task, a service or an app) and what ran inside it, and can turn off the task behind it, with a backup.

## Sweep stale sessions, safely

- **MCP servers and headless browsers your agents left behind** are found too, and a live meter shows the RAM you got back seconds after each sweep.
- **Orphans**: their terminal is closed but they're still running, like the classic zombie localhost:3000.
- **Idle agents in open terminals are never pre-selected**, because they're waiting for you.
- **Editor language servers are locked** (VS Code, Cursor, JetBrains, Visual Studio…).
- **Kills by process tree, never by port.** Each row is re-verified right before ending it, so two projects are never confused.

Dev Cleanup finds leftover dev servers, build daemons and agent sessions across the stacks you actually use, plus the WSL 2 / Docker VM that quietly holds gigabytes.

## Every change can be undone

- **Original values are backed up** in SQLite before the first change.
- **System Restore point** before the first apply.
- **Watchdog** re-applies tweaks that Windows Update silently reverted.
- **Never touches** Windows Update, the Defender engine, or ~60 core services.

## FAQ

### What does RigSweep do?

RigSweep is a free, open-source Windows app for developers who work with AI coding agents. It finds dev servers, build daemons, MCP servers and agent CLIs that keep running after you close a terminal, shows each one with its project, RAM and ports, and ends the ones you pick. It also deletes old node_modules and package caches, empties browser caches and applies reversible Windows tweaks.

### How do I stop node.exe processes that keep running after I close the terminal?

Open Dev Cleanup. Processes whose terminal is gone are marked as orphans and pre-selected, with their whole process tree, project folder and listening ports (like a stuck localhost:3000). Click End selected, and RigSweep ends each tree after checking that the process ID still belongs to the same program.

### Which AI agents and dev stacks does it support?

Claude Code, Codex, Gemini CLI, opencode, Aider and Goose, plus the MCP servers and headless browsers they start. Dev stacks: Node, Bun, Deno, Python, Java, Gradle, Maven, .NET, Ruby, PHP, Rust and Go, and the WSL 2 / Docker VM.

### Is it safe? Can I undo changes?

Nothing is ended or deleted without your confirmation. Every Windows tweak backs up the original value first and can be restored one by one or all at once, and a System Restore point is created before the first change. Editor language servers and core Windows services are locked.

### Is RigSweep free? Does it collect data?

Yes, it's free forever under the MIT license, with no account and no ads. RigSweep asks once whether to share anonymous usage stats (what it cleaned, by type and size, which well-known apps use the most, and which hardware vendor software is installed). Nothing is sent before you answer, the exact report is shown in Settings, and turning it off deletes what was sent. No paths, project names or personal data are ever included. The update check stays off unless you turn it on or click Check now.

### Does it run on macOS or Linux?

No. RigSweep is built for Windows 10 and 11, because it works with Windows services, Task Scheduler and Defender. It can still shut down the WSL 2 VM from the Windows side.

## Did Defender delete it?

RigSweep is open source but not code-signed yet. Tools that change services look like malware to machine-learning heuristics, so Defender may flag it by mistake (e.g. `Trojan:Win32/Bearfoos!ml`, where `!ml` means a heuristic guess).

The installer **asks** whether to exclude only its own folder, and the uninstaller removes that exclusion again. Code signing through SignPath Foundation is planned. [Read the full FAQ →](https://github.com/yobotn/rigsweep#antivirus-false-positives)

## Privacy

Free code signing provided by SignPath.io, certificate by SignPath Foundation (application in progress; current releases are not signed yet). Installers are built from the tagged source by GitHub Actions. No telemetry, no analytics. The only network request is the update check, and only if you turn it on or click Check now. Full policy: https://rigsweep.com/privacy/

## Disclaimer

RigSweep changes Windows services, scheduled tasks, registry policies and Microsoft Defender settings, and it can end running processes. Every tweak is backed up and can be restored, and nothing happens without your confirmation, but you use it at your own risk. It is provided "as is", without warranty of any kind (MIT license). Review each change before applying it, keep your own backups, and don't use it on work- or school-managed devices without your IT team's approval. RigSweep is an independent project, not affiliated with Microsoft, Google, Anthropic, OpenAI or any other tool it mentions; all trademarks belong to their owners.

## Links

- [Guides](https://rigsweep.com/guides/) · [Compare](https://rigsweep.com/compare/) · [API and developer docs](https://rigsweep.com/developers/) · [CLI reference](https://rigsweep.com/cli/) · [About](https://rigsweep.com/about/) · [Privacy](https://rigsweep.com/privacy/) · [Contact](https://rigsweep.com/contact/)
- [Vote on upcoming features](https://github.com/yobotn/rigsweep/issues/12)
- [Report an issue](https://github.com/yobotn/rigsweep/issues)
