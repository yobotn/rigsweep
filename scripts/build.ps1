# Release build that keeps the build machine's paths (e.g. C:\Users\<name>\.cargo\...) out of
# the binary. Rust embeds source paths of dependencies for panic messages; remap them to "~".
# CARGO_ENCODED_RUSTFLAGS (0x1f-separated) is used so paths containing spaces stay intact.
$ErrorActionPreference = 'Stop'
$sep = [char]0x1f
$flags = @("--remap-path-prefix=$($env:USERPROFILE)=~")
if ($env:CARGO_HOME) { $flags += "--remap-path-prefix=$($env:CARGO_HOME)=~/.cargo" }
$repo = Split-Path -Parent $PSScriptRoot
$flags += "--remap-path-prefix=$repo=."
$env:CARGO_ENCODED_RUSTFLAGS = ($flags -join $sep)
Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue
$extra = @()
# With the updater signing key present (release CI), also produce the installer's .sig for the
# opt-in updater. The config path must be absolute: a relative one is silently ignored here.
if ($env:TAURI_SIGNING_PRIVATE_KEY) { $extra += @("--config", (Join-Path $repo "src-tauri/tauri.updater.conf.json")) }
& npx tauri build @extra @args
exit $LASTEXITCODE
