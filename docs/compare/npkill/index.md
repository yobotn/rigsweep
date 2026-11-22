# RigSweep vs npkill

> RigSweep vs npkill compared: npkill is a free MIT terminal tool that finds and deletes node_modules and other build folders; RigSweep is a Windows app that also cleans package caches and ends leftover dev processes.

**npkill is a free, open-source (MIT) terminal tool that finds `node_modules` and other heavy build folders and lets you delete them with one key.** RigSweep is a free, open-source (MIT) Windows desktop app that does this too, and also cleans package caches, ends leftover dev servers and MCP servers, and empties browser caches. If you only want to clear project folders from the terminal, npkill is quicker to start; for the rest of a Windows dev machine, RigSweep covers more.

## Who each is for

- **npkill**: developers on any machine with Node.js who want to clean project folders from a terminal, with `npx npkill` and no install.
- **RigSweep**: developers on Windows who want one app for stale dev processes, project folders, package caches and caches left by AI tools.

## Facts side by side

|   | RigSweep | npkill |
| --- | --- | --- |
| Price and license | Free, MIT license | Free, MIT license |
| Open source | Yes | Yes |
| Platforms | Windows 10 and 11 | Terminal app started with `npx npkill` (needs Node.js). Its docs cover Windows paths |
| Ends leftover dev processes | Yes: dev servers, build daemons, MCP servers, agent CLIs, headless browsers, with their process trees | No |
| node_modules and build output | Yes: Node, Rust, .NET, Java, Python projects, only next to their project file | Yes, with profiles for Node, Python, Java, Android, .NET, Rust, Elixir, Haskell, Scala, C++, Unity, Unreal, Godot, PHP, Dart, Swift, Zig, Terraform and more |
| Package caches | Yes: npm, pnpm, Yarn, Bun, pip, uv, Cargo, Go, NuGet, Gradle, Maven, Playwright, Puppeteer, Hugging Face | Not a listed feature |
| Browser cleanup | Empties caches of Chrome, Edge, Brave, Vivaldi, Opera and Firefox. Sign-ins, cookies, passwords and history are never touched | No |
| Registry cleaner | No | No |
| Driver updater | No | No |
| Ads, upsell, telemetry | No ads, no account, no paid tier. Anonymous usage stats only if you agree at first run | – |
| Interface languages | 11 | – |

Checked November 2026 on npkill's official repository ([README](https://github.com/voidcosmos/npkill) and [profiles](https://github.com/voidcosmos/npkill/blob/main/docs/profiles.md)). RigSweep facts are from its [README](https://github.com/yobotn/rigsweep#readme) and [homepage](https://rigsweep.com/). A dash means we couldn't verify it on an official source. Spotted something out of date? [Open an issue](https://github.com/yobotn/rigsweep/issues).

## Where npkill is the better choice

- You live in the terminal and want to start cleaning in seconds with `npx npkill`.
- You use stacks RigSweep doesn't scan, such as Unity, Unreal, Godot, Elixir, Haskell, Dart, Zig or Terraform.
- You want to script it: npkill has `--json` output, `--dry-run`, `--older-than` and `--delete-all`.
- You're on macOS or Linux, where RigSweep doesn't run.

## Where RigSweep is the better choice

- You also want package caches cleaned (npm, pnpm, Yarn, Bun, pip, uv, Cargo, Go, NuGet, Gradle, Maven, Playwright, Hugging Face), with uv and Go cleaned through their own commands.
- You want dev servers, MCP servers and agent CLIs left running after a terminal closed found and ended.
- You want projects a running process is using locked, so a dev server's files aren't deleted under it.
- You prefer a desktop app with sizes, project ages and a confirmation step over a terminal UI.

More on doing this by hand: [Delete node_modules and dev caches on Windows](https://rigsweep.com/guides/delete-node-modules-and-dev-caches-windows/). [Download RigSweep](https://github.com/yobotn/rigsweep/releases/latest).

## FAQ

### Is npkill safe to use?

npkill finds candidate folders and leaves the decision to you. It marks folders it thinks are sensitive with a warning and hides them by default. Deleting a dependency or build folder is safe when the project can reinstall or rebuild it.

### Does RigSweep replace npkill on Windows?

For Node, Rust, .NET, Java and Python projects, yes, and it adds package caches and dev process cleanup. npkill supports more stacks and is easier to script.

### Do npkill and RigSweep clean the npm cache?

RigSweep cleans the npm cache and other package caches. npkill's documentation doesn't list package caches; it focuses on folders inside projects.
