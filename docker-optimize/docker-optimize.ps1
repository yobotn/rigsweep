<#
.SYNOPSIS
  RigSweep one-time tuning for Docker Desktop + WSL2. Reversible, never deletes data.

.DESCRIPTION
  Default (no switch) : dry run - shows what would change, touches nothing.
  -Apply              : backs up, writes .wslconfig + Docker Desktop settings,
                        restarts WSL/Docker, restarts the containers that were running.
  -Revert             : restores the backups and restarts the same way.

  Containers, images and volumes are never removed. No prune, no VHD compaction.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File docker-optimize\docker-optimize.ps1
  powershell -ExecutionPolicy Bypass -File docker-optimize\docker-optimize.ps1 -Apply
  powershell -ExecutionPolicy Bypass -File docker-optimize\docker-optimize.ps1 -Revert
#>
[CmdletBinding()]
param(
    [switch]$Apply,
    [switch]$Revert,
    [int]$MemoryGB = 0,      # 0 = auto (RAM/4, clamped 4..16)
    [int]$Processors = 0,    # 0 = auto (half the logical cores, min 2)
    [int]$SwapGB = 2,
    [switch]$Yes             # skip the confirmation prompt
)

$ErrorActionPreference = 'Stop'
if ($Apply -and $Revert) { throw 'Use either -Apply or -Revert, not both.' }

# ---------------------------------------------------------------- paths
$WslConfig     = Join-Path $env:USERPROFILE '.wslconfig'
$DockerStore   = Join-Path $env:APPDATA 'Docker\settings-store.json'
$BackupDir     = Join-Path $env:USERPROFILE '.rigsweep\docker-wsl-backup'
$BakWsl        = Join-Path $BackupDir 'wslconfig.bak'
$BakWslNone    = Join-Path $BackupDir 'wslconfig.none'   # marker: there was no .wslconfig
$BakStore      = Join-Path $BackupDir 'settings-store.json.bak'
$BakRunning    = Join-Path $BackupDir 'running-containers.txt'

# Docker Desktop settings we change (names verified against Docker Desktop 4.91)
$DockerWanted = [ordered]@{
    UseResourceSaver  = $true    # stop the VM when no container runs
    KubernetesEnabled = $false   # big idle cost if unused
    ExtensionsEnabled = $false
    SbomIndexing      = $false   # background image scanning
    EnableDockerAI    = $false   # "Ask Gordon" assistant
}

# ---------------------------------------------------------------- helpers
function Write-Step($t) { Write-Host ""; Write-Host "== $t" -ForegroundColor Cyan }
function Write-Info($t) { Write-Host "   $t" }
function Write-Warn2($t) { Write-Host "   ! $t" -ForegroundColor Yellow }
function Write-Ok($t)   { Write-Host "   OK $t" -ForegroundColor Green }

function Write-Utf8NoBom([string]$Path, [string]$Text) {
    $dir = Split-Path $Path -Parent
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Force $dir | Out-Null }
    [IO.File]::WriteAllText($Path, $Text, (New-Object Text.UTF8Encoding($false)))
}

function Test-DockerUp {
    & docker info --format '{{.ServerVersion}}' 2>$null | Out-Null
    return ($LASTEXITCODE -eq 0)
}

function Get-VmMemMB {
    $p = Get-Process -Name 'vmmemWSL','vmmem' -ErrorAction SilentlyContinue
    if (-not $p) { return 0 }
    return [int](($p | Measure-Object WorkingSet64 -Sum).Sum / 1MB)
}

function Get-DockerCounts {
    if (-not (Test-DockerUp)) { return $null }
    $c = @(& docker ps -aq 2>$null).Count
    $r = @(& docker ps -q 2>$null).Count
    $v = @(& docker volume ls -q 2>$null).Count
    $i = @(& docker images -q 2>$null | Sort-Object -Unique).Count
    return [pscustomobject]@{ Containers = $c; Running = $r; Volumes = $v; Images = $i }
}

function Get-DockerDesktopExe {
    $p = Get-Process -Name 'Docker Desktop' -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($p -and $p.Path) { return $p.Path }
    $cands = @(
        (Join-Path $env:LOCALAPPDATA 'Programs\DockerDesktop\frontend\Docker Desktop.exe'),
        (Join-Path $env:ProgramFiles 'Docker\Docker\Docker Desktop.exe')
    )
    foreach ($c in $cands) { if (Test-Path $c) { return $c } }
    return $null
}

# --- .wslconfig: minimal INI merge that keeps every line we do not own
function Merge-Ini([string]$Text, [System.Collections.IDictionary]$Wanted) {
    # $Wanted: @{ 'wsl2' = [ordered]@{key=val}; 'experimental' = ... }
    $lines = New-Object System.Collections.Generic.List[string]
    if ($Text) { $Text -split "`r?`n" | ForEach-Object { $lines.Add($_) } }
    while ($lines.Count -gt 0 -and $lines[$lines.Count - 1] -eq '') { $lines.RemoveAt($lines.Count - 1) }

    foreach ($section in $Wanted.Keys) {
        $keys = $Wanted[$section]
        # locate section
        $start = -1
        for ($i = 0; $i -lt $lines.Count; $i++) {
            if ($lines[$i].Trim() -match ('^\[' + [regex]::Escape($section) + '\]$')) { $start = $i; break }
        }
        if ($start -lt 0) {
            if ($lines.Count -gt 0) { $lines.Add('') }
            $lines.Add("[$section]")
            foreach ($k in $keys.Keys) { $lines.Add("$k=$($keys[$k])") }
            continue
        }
        $end = $lines.Count
        for ($i = $start + 1; $i -lt $lines.Count; $i++) {
            if ($lines[$i].Trim() -match '^\[.+\]$') { $end = $i; break }
        }
        foreach ($k in $keys.Keys) {
            $found = $false
            for ($i = $start + 1; $i -lt $end; $i++) {
                if ($lines[$i] -match ('^\s*' + [regex]::Escape($k) + '\s*=')) {
                    $lines[$i] = "$k=$($keys[$k])"; $found = $true; break
                }
            }
            if (-not $found) {
                # insert after last non-empty line of the section
                $ins = $end
                while ($ins -gt $start + 1 -and $lines[$ins - 1].Trim() -eq '') { $ins-- }
                $lines.Insert($ins, "$k=$($keys[$k])"); $end++
            }
        }
    }
    return (($lines -join "`r`n") + "`r`n")
}

function Stop-DockerDesktop {
    Write-Info 'Stopping Docker Desktop...'
    & docker desktop stop 2>$null | Out-Null
    $deadline = (Get-Date).AddSeconds(60)
    while ((Get-Process -Name 'Docker Desktop' -ErrorAction SilentlyContinue) -and (Get-Date) -lt $deadline) {
        Start-Sleep -Seconds 2
    }
    if (Get-Process -Name 'Docker Desktop' -ErrorAction SilentlyContinue) {
        Write-Warn2 'Graceful stop timed out, closing the app.'
        Get-Process -Name 'Docker Desktop' -ErrorAction SilentlyContinue | Stop-Process -Force
        Start-Sleep -Seconds 3
    }
}

function Start-DockerDesktop([string]$Exe) {
    Write-Info 'Starting Docker Desktop...'
    & docker desktop start 2>$null | Out-Null
    if ($LASTEXITCODE -ne 0 -and $Exe) { Start-Process -FilePath $Exe | Out-Null }
    $deadline = (Get-Date).AddSeconds(240)
    while (-not (Test-DockerUp)) {
        if ((Get-Date) -gt $deadline) { throw 'Docker did not come back within 4 minutes. Start Docker Desktop manually, then run: docker start <names from ' + $BakRunning + '>' }
        Start-Sleep -Seconds 3
    }
    Write-Ok 'Docker engine is up.'
}

function Restart-Stack([string]$Exe) {
    Write-Step 'Restarting WSL and Docker'
    Write-Info 'wsl --shutdown'
    & wsl --shutdown
    Start-Sleep -Seconds 3
    Start-DockerDesktop $Exe
    if (Test-Path $BakRunning) {
        $names = @(Get-Content $BakRunning | Where-Object { $_.Trim() })
        foreach ($n in $names) {
            & docker start $n 2>$null | Out-Null
            if ($LASTEXITCODE -eq 0) { Write-Ok "started $n" } else { Write-Warn2 "could not start $n" }
        }
    }
}

function Confirm-Go([string]$Msg) {
    if ($Yes) { return }
    $a = Read-Host "$Msg [y/N]"
    if ($a -notmatch '^(y|yes)$') { Write-Host 'Cancelled, nothing changed.'; exit 1 }
}

# ================================================================ Phase 0: inspect
Write-Host 'RigSweep docker-optimize - one-time Docker Desktop / WSL tuning' -ForegroundColor White
$mode = 'DRY RUN (nothing will change)'
if ($Apply) { $mode = 'APPLY' } elseif ($Revert) { $mode = 'REVERT' }
Write-Host "Mode: $mode"

Write-Step 'Machine'
$cs    = Get-CimInstance Win32_ComputerSystem
$ramGB = [math]::Round($cs.TotalPhysicalMemory / 1GB)
$cores = [int]$cs.NumberOfLogicalProcessors
Write-Info "RAM $ramGB GB, $cores logical CPUs"

$wslVer = ''
try { $wslVer = (& wsl --version 2>$null | Out-String) -replace "`0", '' } catch {}
$m = [regex]::Match($wslVer, '(\d+)\.(\d+)\.(\d+)')
if ($m.Success) {
    Write-Info "WSL $($m.Value)"
    if ([int]$m.Groups[1].Value -lt 2) { throw 'WSL 2.0+ is required (run: wsl --update).' }
} else {
    throw 'Could not read WSL version (run: wsl --update).'
}

$exe = Get-DockerDesktopExe
if (-not $exe) { throw 'Docker Desktop not found.' }
Write-Info "Docker Desktop: $exe"

$dockerUp = Test-DockerUp
$before   = Get-DockerCounts
$vmBefore = Get-VmMemMB
if ($before) {
    Write-Info ("Containers {0} (running {1}), volumes {2}, images {3}" -f $before.Containers, $before.Running, $before.Volumes, $before.Images)
} else {
    Write-Warn2 'Docker engine not running - container list unavailable.'
}
Write-Info "WSL VM memory now: $vmBefore MB"

$vhd = Get-ChildItem (Join-Path $env:LOCALAPPDATA 'Docker\wsl') -Recurse -Filter *.vhdx -ErrorAction SilentlyContinue
foreach ($f in $vhd) { Write-Info ("Disk {0}: {1:N1} GB" -f $f.Name, ($f.Length / 1GB)) }

# ================================================================ REVERT
if ($Revert) {
    if (-not ((Test-Path $BakWsl) -or (Test-Path $BakWslNone) -or (Test-Path $BakStore))) {
        throw "No backup found in $BackupDir"
    }
    Write-Step 'Revert plan'
    if (Test-Path $BakWsl)     { Write-Info "restore .wslconfig from backup" }
    if (Test-Path $BakWslNone) { Write-Info "delete .wslconfig (there was none before)" }
    if (Test-Path $BakStore)   { Write-Info "restore Docker settings-store.json from backup" }
    Write-Warn2 'Running containers stop briefly and are restarted afterwards.'
    Confirm-Go 'Revert now?'

    if ($dockerUp) { @(& docker ps --format '{{.Names}}') | Set-Content -Encoding ascii $BakRunning }
    Stop-DockerDesktop
    if (Test-Path $BakWsl)     { Copy-Item $BakWsl $WslConfig -Force }
    if (Test-Path $BakWslNone) { Remove-Item $WslConfig -Force -ErrorAction SilentlyContinue }
    if (Test-Path $BakStore)   { Copy-Item $BakStore $DockerStore -Force }
    Restart-Stack $exe
    Write-Step 'Done'
    Write-Ok 'Previous configuration restored.'
    exit 0
}

# ================================================================ compute plan
if ($MemoryGB -le 0)   { $MemoryGB = [math]::Max(4, [math]::Min(16, [math]::Floor($ramGB / 4))) }
if ($Processors -le 0) { $Processors = [math]::Max(2, [math]::Floor($cores / 2)) }

$iniWanted = [ordered]@{
    'wsl2'         = [ordered]@{ memory = "${MemoryGB}GB"; processors = "$Processors"; swap = "${SwapGB}GB" }
    'experimental' = [ordered]@{ autoMemoryReclaim = 'gradual'; sparseVhd = 'true' }
}
$oldIni = ''
if (Test-Path $WslConfig) { $oldIni = [IO.File]::ReadAllText($WslConfig) }
$newIni = Merge-Ini $oldIni $iniWanted

Write-Step "Plan: $WslConfig"
if (-not $oldIni) { Write-Info '(file does not exist yet - will be created)' }
if ($oldIni -eq $newIni) {
    Write-Ok 'already tuned, no change'
} else {
    $newIni -split "`r?`n" | Where-Object { $_ } | ForEach-Object { Write-Info "  $_" }
}

Write-Step "Plan: Docker Desktop settings"
$store = $null
if (Test-Path $DockerStore) { $store = Get-Content $DockerStore -Raw | ConvertFrom-Json }
if (-not $store) { $store = New-Object psobject }
$storeChanges = @()
foreach ($k in $DockerWanted.Keys) {
    $want = $DockerWanted[$k]
    $prop = $store.PSObject.Properties[$k]
    $cur  = '(default)'
    if ($prop) { $cur = $prop.Value }
    if ($prop -and $prop.Value -eq $want) { Write-Ok "$k = $want"; continue }
    Write-Info ("{0}: {1} -> {2}" -f $k, $cur, $want)
    $storeChanges += $k
}

if ($before -and $before.Running -gt 0) {
    Write-Step 'Running containers (stop during restart, restarted by this script)'
    $ids = @(& docker ps -q)
    foreach ($id in $ids) {
        $info = & docker inspect --format '{{.Name}}|{{.HostConfig.RestartPolicy.Name}}' $id
        $parts = $info.TrimStart('/') -split '\|'
        $pol = $parts[1]; if (-not $pol) { $pol = 'no' }
        Write-Info ("{0}  (restart policy: {1})" -f $parts[0], $pol)
    }
}

$nothingToDo = ($oldIni -eq $newIni) -and ($storeChanges.Count -eq 0)

if (-not $Apply) {
    Write-Step 'Dry run finished'
    if ($nothingToDo) { Write-Ok 'Everything is already tuned.' }
    else { Write-Info 'Run again with -Apply to make these changes (reversible with -Revert).' }
    exit 0
}

if ($nothingToDo) { Write-Step 'Done'; Write-Ok 'Everything is already tuned.'; exit 0 }

# ================================================================ Phase 1: backup
Confirm-Go 'Apply now? Docker and WSL will restart; running containers come back automatically.'

Write-Step 'Backup'
New-Item -ItemType Directory -Force $BackupDir | Out-Null
if (-not ((Test-Path $BakWsl) -or (Test-Path $BakWslNone))) {
    # keep the ORIGINAL state across repeated -Apply runs
    if (Test-Path $WslConfig) { Copy-Item $WslConfig $BakWsl -Force; Write-Ok "saved $BakWsl" }
    else { Set-Content -Path $BakWslNone -Value 'no .wslconfig before RigSweep' -Encoding ascii; Write-Ok 'recorded: no .wslconfig before' }
} else { Write-Info 'original .wslconfig backup already present, kept' }
if ((Test-Path $DockerStore) -and -not (Test-Path $BakStore)) {
    Copy-Item $DockerStore $BakStore -Force; Write-Ok "saved $BakStore"
} elseif (Test-Path $BakStore) { Write-Info 'original Docker settings backup already present, kept' }

if ($dockerUp) {
    @(& docker ps --format '{{.Names}}') | Set-Content -Encoding ascii $BakRunning
} else {
    Set-Content -Encoding ascii $BakRunning -Value ''
}
Write-Ok "running containers list: $BakRunning"

# ================================================================ Phase 2: write
# Docker Desktop rewrites settings-store.json on exit, so stop it first.
Stop-DockerDesktop

Write-Step 'Writing configuration'
if ($oldIni -ne $newIni) { Write-Utf8NoBom $WslConfig $newIni; Write-Ok '.wslconfig written' }
if ($storeChanges.Count -gt 0) {
    # re-read: the app may have saved on exit
    $store = $null
    if (Test-Path $DockerStore) { $store = Get-Content $DockerStore -Raw | ConvertFrom-Json }
    if (-not $store) { $store = New-Object psobject }
    foreach ($k in $DockerWanted.Keys) {
        $store | Add-Member -NotePropertyName $k -NotePropertyValue $DockerWanted[$k] -Force
    }
    Write-Utf8NoBom $DockerStore ($store | ConvertTo-Json -Depth 32)
    Write-Ok 'Docker settings written'
}

# ================================================================ Phase 3: restart
Restart-Stack $exe

# ================================================================ Phase 4: verify
Write-Step 'Verify'
Start-Sleep -Seconds 5
$after   = Get-DockerCounts
$vmAfter = Get-VmMemMB
if ($before -and $after) {
    Write-Info ("Containers {0} -> {1}, running {2} -> {3}, volumes {4} -> {5}, images {6} -> {7}" -f `
        $before.Containers, $after.Containers, $before.Running, $after.Running, `
        $before.Volumes, $after.Volumes, $before.Images, $after.Images)
    if ($after.Containers -eq $before.Containers -and $after.Volumes -eq $before.Volumes -and $after.Images -eq $before.Images) {
        Write-Ok 'all containers, volumes and images present'
    } else {
        Write-Warn2 'counts differ - check docker ps -a / docker volume ls'
    }
    if ($after.Running -lt $before.Running) { Write-Warn2 'some containers are not running again - see list above' }
}
Write-Info "WSL VM memory: $vmBefore MB -> $vmAfter MB (cap now ${MemoryGB} GB)"

Write-Step 'Done'
Write-Ok 'Tuning applied. Undo any time with: -Revert'
