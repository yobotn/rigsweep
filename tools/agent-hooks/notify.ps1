<#
.SYNOPSIS
  notify.ps1 -Event done|failed|attention [-Message text]
  Opt-in: does nothing unless $env:PES_NOTIFY = '1'. $env:PES_SOUND = '0' disables sound.
  Never blocks; always exits 0; sends nothing off the machine.
#>
param([string]$Event = "done", [string]$Message = "")
if ($env:PES_NOTIFY -ne '1') { exit 0 }
try {
  $title = switch ($Event) { 'failed' { 'Task failed' } 'attention' { 'Needs your input' } default { 'Task finished' } }
  if (-not $Message) { $Message = Split-Path -Leaf (Get-Location) }

  if (Get-Module -ListAvailable -Name BurntToast) {
    Import-Module BurntToast; New-BurntToastNotification -Text $title, $Message -Silent
  } else {
    Add-Type -AssemblyName System.Windows.Forms, System.Drawing
    $n = New-Object System.Windows.Forms.NotifyIcon
    $n.Icon = [System.Drawing.SystemIcons]::Information
    $n.BalloonTipTitle = $title; $n.BalloonTipText = $Message; $n.Visible = $true
    $n.ShowBalloonTip(4000); Start-Sleep -Milliseconds 500; $n.Dispose()
  }
  if ($env:PES_SOUND -ne '0') {
    switch ($Event) {
      'failed'    { [System.Media.SystemSounds]::Hand.Play() }
      'attention' { [System.Media.SystemSounds]::Exclamation.Play() }
      default     { [System.Media.SystemSounds]::Asterisk.Play() }
    }
  }
} catch { }
exit 0
