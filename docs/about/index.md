# About RigSweep

> Why RigSweep exists, who builds it, and how it is made: a free, open-source Windows app for developers who work with AI coding agents.

**RigSweep is a free, open-source Windows app that gives your machine room back from AI-agent sessions.** It finds dev servers, build daemons, MCP servers and agent CLIs that kept running after a terminal was closed, deletes stale `node_modules` and package caches, and applies reversible Windows tweaks.

## Why it exists

In the AI era a dev laptop doesn't run one thing anymore. A normal day means several Claude Code sessions in different terminals, a dev server per project, build watchers and a browser full of docs and previews. Every session spawns more processes, and many keep running after you close the terminal. On the author's 32 GB laptop, a mid-day measurement found 37 Chrome processes using 6.5 GB and 14 Node and Claude processes using 2.5 GB. None of it was malware. It was the normal leftovers of working with agents.

Classic "PC booster" apps either do scary, irreversible things or know nothing about dev workflows. RigSweep was built to do the opposite: show what is actually using the machine, sweep what is stale, and keep every change reversible.

## Principles

- **Nothing happens without confirmation.** No process is ended and no folder is deleted until you click.
- **Everything is reversible.** Original values are backed up before any Windows tweak, and a System Restore point is created first.
- **Measured, not marketed.** The Boost Report compares real snapshots instead of showing fake percentages.
- **Private by design.** No account and no ads. Anonymous usage stats are shared only after you answer a first-run question, never with paths or names, and you can turn them off at any time. See the [privacy policy](https://rigsweep.com/privacy/).

## Who builds it

RigSweep is an independent open-source project maintained by [@yobotn](https://github.com/yobotn), written in Rust with Tauri 2 and released under the MIT license. Releases are built from tagged source by GitHub Actions. It is not affiliated with Microsoft, Google, Anthropic, OpenAI or any other tool it mentions.

## Get involved

Pick what gets built next on the [feature vote](https://github.com/yobotn/rigsweep/issues/12), report bugs in [Issues](https://github.com/yobotn/rigsweep/issues), share your before/after results in [Discussions](https://github.com/yobotn/rigsweep/discussions), or improve a translation with a pull request.
