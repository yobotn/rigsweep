# Security policy

## Reporting a vulnerability

Please report security problems **privately** through GitHub:
[Report a vulnerability](https://github.com/yobotn/rigsweep/security/advisories/new).
Don't open a public issue for them.

You'll get a reply within a few days. Once a fix is released, the advisory is published with credit, if you
want it.

## What's in scope

- The RigSweep app and installer (it runs elevated, so privilege or command-injection issues matter most)
- The update check and installer signature verification
- The CLI and MCP server mode (`--disk-report`, `--disk-clean`, `--mcp`)
- The Chrome extension
- The stats API at `stats.rigsweep.com` and the website at `rigsweep.com`

Please don't run automated scanners or load tests against `rigsweep.com` or `stats.rigsweep.com`; a local
build of `server/` works for testing (see `server/deploy/README.md`).

## Supported versions

Only the latest release gets fixes. The in-app update check (Settings → Updates) installs it after verifying
its signature.

## Antivirus false positives

RigSweep isn't code-signed yet, so Defender may flag it by mistake. That isn't a vulnerability; see
[Antivirus false positives](https://github.com/yobotn/rigsweep#antivirus-false-positives).
