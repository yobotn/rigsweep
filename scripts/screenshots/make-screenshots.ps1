# Renders the README / landing-page Dev Cleanup screenshots from the real UI with made-up demo
# data (mock-tauri.js), in headless Edge (the same engine as the app's WebView2).
# Usage: powershell -ExecutionPolicy Bypass -File scripts\screenshots\make-screenshots.ps1
$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$repo = Resolve-Path (Join-Path $here "..\..")
$out = Join-Path $repo "docs\screenshots"
$work = Join-Path $env:TEMP "rigsweep-shots"

$edge = @(
  "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe",
  "$env:ProgramFiles\Microsoft\Edge\Application\msedge.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $edge) { throw "Microsoft Edge not found" }

if (Test-Path $work) { Remove-Item -Recurse -Force $work }
New-Item -ItemType Directory -Force "$work\ui" | Out-Null
Copy-Item -Recurse (Join-Path $repo "ui\*") "$work\ui"
Copy-Item (Join-Path $here "mock-tauri.js"), (Join-Path $here "mock-after.js") "$work\ui"

$index = "$work\ui\index.html"
$html = [IO.File]::ReadAllText($index)
$html = $html.Replace('<script src="i18n.js"></script>', '<script src="mock-tauri.js"></script><script src="i18n.js"></script>')
$html = $html.Replace('<script src="app.js"></script>', '<script src="app.js"></script><script src="mock-after.js"></script>')
[IO.File]::WriteAllText($index, $html, (New-Object Text.UTF8Encoding $false))

$url = ([Uri]$index).AbsoluteUri
foreach ($lang in "en", "de", "es", "fr", "it", "pt-BR", "ru", "ja", "ko", "zh-CN") {
  $name = if ($lang -eq "en") { "cleanup.png" } else { "cleanup-$lang.png" }
  $png = Join-Path $out $name
  # separate profile so an open Edge window is never reused
  $args = @("--headless=new", "--disable-gpu", "--hide-scrollbars", "--force-device-scale-factor=1",
    "--user-data-dir=$work\profile-$lang", "--window-size=1240,820", "--virtual-time-budget=4000",
    "--screenshot=$png", "$url`?lang=$lang&tab=cleanup")
  Start-Process -FilePath $edge -ArgumentList $args -Wait -WindowStyle Hidden
  Write-Host "wrote $png"
}
