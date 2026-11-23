# 0003: How a release is trusted before code signing

- **Status:** accepted
- **Date:** 2026-11-23
- **Deciders:** @yobotn

## Context

The installer isn't Authenticode-signed yet (SignPath Foundation application pending), so Windows SmartScreen
warns and Defender's machine-learning heuristics sometimes flag it. Users and the in-app updater still need a way
to know an installer really came from this repository's tagged source.

## Decision

Every release, built only by `.github/workflows/release.yml` from a `v*` tag, without build caches, carries:

- an **updater signature** (`.sig`, Tauri's minisign key in the `TAURI_SIGNING_PRIVATE_KEY` secret); the app
  verifies it before installing an update,
- a **SHA-256** file,
- a **build provenance attestation**: `gh attestation verify RigSweep_<v>_x64-setup.exe -R yobotn/rigsweep`,
- a **CycloneDX SBOM** (`rigsweep-sbom.cdx.json`) of the Rust and npm dependencies.

Release notes are written by hand in `.release/notes-<version>.md` and published with `gh release create`
right after the tag push, so `latest.json` carries them. They are the changelog (no `CHANGELOG.md`).

## Consequences

- When SignPath signing arrives, the signed installer replaces the unsigned artifact; the rest stays.
- Rotating the updater key needs a release signed with the old key that ships the new public key.

## Challenge log

- *"Generate a changelog from Conventional Commits?"* Commits here are plain sentences written for developers;
  users get hand-written notes. Recorded as an exception.
