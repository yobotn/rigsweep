# Delete node_modules, build output and package caches on Windows

> How to reclaim disk space from node_modules, .next, target, bin/obj, .venv and the npm, pnpm, Yarn, pip, uv, Cargo, Go, NuGet, Gradle, Maven, Playwright and Hugging Face caches on Windows, with each tool's own command and the caveats.

**Dependency folders, build output and package caches are safe to delete when you can rebuild or re-download them.** Delete project folders like `node_modules`, `target` or `bin/obj` for projects you aren't working on, and clean package caches with each tool's own command (`npm cache clean --force`, `pnpm store prune`, `uv cache prune`, `go clean -cache`). The next install or build brings back what it needs, at the cost of time and bandwidth.

## Project folders

| Folder | Made by | How it comes back |
| --- | --- | --- |
| `node_modules` | npm, pnpm, Yarn, Bun | `npm install` (or your package manager's install) |
| `.next`, `.nuxt`, `dist` | Next.js, Nuxt, bundlers | The next build or dev run |
| `target` | Cargo (Rust), Maven (Java) | `cargo build`, `mvn package`. In a Rust project, `cargo clean` removes it |
| `bin`, `obj` | .NET | `dotnet build` |
| `build`, `.gradle` | Gradle | The next Gradle build |
| `.venv`, `venv` | Python | Recreate it, for example `uv sync` or `python -m venv .venv` plus `pip install -r requirements.txt` |

Only delete a folder when the project file that produced it sits next to it (`package.json`, `Cargo.toml`, `*.csproj`, `pyvenv.cfg`). A folder called `build` or `bin` elsewhere may hold something you can't rebuild. On Windows, `rd` often handles deep `node_modules` trees better than Explorer:

```
cmd /c rd /s /q node_modules
```

## Package caches, with each tool's own command

| Tool | Clean command | Note |
| --- | --- | --- |
| npm | `npm cache clean --force` | `npm cache verify` checks and trims the cache without emptying it |
| pnpm | `pnpm store prune` | Removes only packages no project references. `pnpm store path` shows where the store is |
| Yarn | `yarn cache clean` | `yarn cache dir` shows the folder |
| pip | `pip cache purge` | `pip cache dir` shows the folder |
| uv | `uv cache prune` | Removes unused entries. `uv cache clean` empties the whole cache in `%LOCALAPPDATA%\uv\cache` |
| Go | `go clean -cache`, `go clean -modcache` | The build cache rebuilds itself. The module cache means re-downloading every module |
| Cargo | `cargo clean` (per project) | Recent Cargo versions clean old downloads in the Cargo home cache on their own. Most of the space is usually in each project's `target` |
| NuGet | `dotnet nuget locals all --clear` | `dotnet nuget locals all --list` shows the folders first |
| Gradle | `gradle --stop`, then delete `%USERPROFILE%\.gradle\caches` | Stop the daemons first or files stay locked. Everything is downloaded again |
| Maven | Delete folders in `%USERPROFILE%\.m2\repository` | See the caveat below about `mvn install` |
| Playwright | `npx playwright uninstall` | Add `--all` to also remove browsers from other Playwright installs. Default folder: `%USERPROFILE%\AppData\Local\ms-playwright` |
| Hugging Face | `hf cache ls`, `hf cache rm <repo>`, `hf cache prune` | Models can be many GB to download again. `--dry-run` previews |

## Caveats

- **Maven local installs can't be re-downloaded.** Artifacts you built yourself with `mvn install` exist only in `.m2\repository`. Deleting the whole folder removes them until you build those projects again.
- **pnpm uses hard links.** Files in a project's `node_modules` and in the pnpm store are the same bytes on disk, so Explorer counts them twice. Deleting one project's `node_modules` frees less than its size suggests, and `pnpm store prune` is the way to free store space.
- **Locked files.** A running dev server, watcher, test runner or editor can hold files open (native `.node` modules, `esbuild.exe`). Deletion then fails part way. Stop the process first; the [leftover processes guide](https://rigsweep.com/guides/claude-code-leftover-processes-windows/) shows how to find it.
- **Long paths.** Deep `node_modules` trees can exceed the classic 260-character path limit. `rd /s /q` or a package-manager command usually works where Explorer stops.
- **Big re-downloads.** Playwright browsers, Hugging Face models, Gradle, Maven, NuGet and Go module caches take time and bandwidth to fill again. Clean them when you need the space, not by habit.

## How RigSweep's Dev Disk does it

[RigSweep](https://rigsweep.com/) is a free, open-source Windows app. Its Dev Disk tab scans your user profile and lists project folders, package caches, GPU and ML caches, AI tool caches and leftover git worktrees with their size.

- A project folder only counts if its project file sits next to it, so a random `build` folder is never touched.
- Only projects nobody has edited for 30 days (adjustable) are pre-selected. Projects a running process is using are locked.
- uv and Go caches are cleaned with their own commands. For Playwright and Puppeteer, only older browser versions go and the newest stays.
- Caches that mean a big re-download (Playwright, Puppeteer, Hugging Face, Gradle, Maven, NuGet packages, Go modules) start switched off.
- Maven artifacts built with `mvn install` are never deleted.
- Nothing is deleted until you confirm, and every folder is checked again right before deletion. A long cleanup can be paused, resumed or stopped.

Prefer a terminal tool for `node_modules` only? See [RigSweep vs npkill](https://rigsweep.com/compare/npkill/). [Download RigSweep](https://github.com/yobotn/rigsweep/releases/latest) (Windows 10 and 11, MIT).

## FAQ

### Is it safe to delete node_modules?

Yes, if the project has a `package.json` and a lockfile. The next `npm install` (or pnpm, Yarn or Bun install) restores it. Close dev servers and editors using the project first so no files are locked.

### Is it safe to clear the npm cache?

Yes. `npm cache clean --force` empties it and npm downloads packages again when needed. The only cost is time and bandwidth on the next install.

### How do I clean the pnpm store?

Run `pnpm store prune`. It removes packages that no project references and doesn't affect your projects.

### Can I delete the .m2 repository folder?

You can, but artifacts you built with `mvn install` live only there and come back only when you build those projects again. Everything downloaded from a remote repository is downloaded again.
