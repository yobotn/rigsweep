<#
.SYNOPSIS
  Report the four session states (task, safe-to-close, committed, pushed).
.EXAMPLE
  ./session-status.ps1 -ExitCode $LASTEXITCODE
.NOTES
  Exit: 0 = fully synced, 1 = not committed or not pushed, 2 = not a git repo.
#>
param(
  [string]$Path = ".",
  [Nullable[int]]$ExitCode = $null,
  [switch]$Json
)

Push-Location $Path
try {
  git rev-parse --is-inside-work-tree 2>$null | Out-Null
  if ($LASTEXITCODE -ne 0) { Write-Output "Not a git repository: nothing to sync."; exit 2 }

  $repo     = Split-Path -Leaf (git rev-parse --show-toplevel)
  $branch   = git symbolic-ref --short -q HEAD; if (-not $branch) { $branch = "(detached)" }
  $status   = @(git status --porcelain=v1)
  $dirty    = ($status | Where-Object { $_ }).Count
  $untracked= ($status | Where-Object { $_.StartsWith('??') }).Count
  $stashes  = @(git stash list).Count
  $upstream = git rev-parse --abbrev-ref --symbolic-full-name '@{u}' 2>$null
  if ($LASTEXITCODE -ne 0) { $upstream = "" }
  $ahead = 0; $behind = 0
  if ($upstream) {
    $counts = (git rev-list --left-right --count "$upstream...HEAD") -split '\s+'
    $behind = [int]$counts[0]; $ahead = [int]$counts[1]
  }
  $gitDir = git rev-parse --git-dir
  $inProgress = ""
  foreach ($f in 'MERGE_HEAD','REBASE_HEAD','CHERRY_PICK_HEAD','REVERT_HEAD') { if (Test-Path (Join-Path $gitDir $f)) { $inProgress = $f } }
  if ((Test-Path (Join-Path $gitDir 'rebase-merge')) -or (Test-Path (Join-Path $gitDir 'rebase-apply'))) { $inProgress = 'REBASE' }

  $synced = ($dirty -eq 0) -and $upstream -and ($ahead -eq 0) -and (-not $inProgress)

  if ($inProgress)      { $verdict = "ATTENTION: a $inProgress is in progress. Finish or abort it before closing." }
  elseif ($dirty -gt 0) { $verdict = "NOT SAVED: $dirty uncommitted change(s). Next: git add -A; git commit; git push" }
  elseif (-not $upstream){ $verdict = "NOT PUSHED: no upstream. Next: git push -u origin $branch" }
  elseif ($ahead -gt 0) { $verdict = "NOT PUSHED: $ahead local commit(s). Next: git push" }
  else                  { $verdict = "SYNCED: all work is committed and pushed." }
  if ($behind -gt 0)    { $verdict += " (remote has $behind new commit(s): git pull)" }

  $jobsInfo = "not checked (run from your shell: Get-Job)"

  if ($Json) {
    [ordered]@{ repo=$repo; branch=$branch; task_exit=$ExitCode; uncommitted=$dirty; untracked=$untracked;
      upstream=$upstream; ahead=$ahead; behind=$behind; stashes=$stashes; in_progress=$inProgress; synced=[bool]$synced } |
      ConvertTo-Json -Compress
  } else {
    $task = "unknown"
    if ($null -ne $ExitCode) { $task = if ($ExitCode -eq 0) { "finished (exit 0)" } else { "FAILED (exit $ExitCode)" } }
    $tree = if ($dirty -eq 0) { "clean" } else { "$dirty uncommitted change(s), $untracked untracked" }
    Write-Output "Session status: $repo ($branch)"
    Write-Output "  Task           : $task"
    Write-Output "  Working tree   : $tree"
    Write-Output "  Upstream       : $(if ($upstream) { $upstream } else { 'none' })"
    Write-Output "  Local commits  : $ahead ahead, $behind behind"
    Write-Output "  Stashes        : $stashes"
    Write-Output "  Background jobs: $jobsInfo"
    Write-Output "  Verdict        : $verdict"
  }
  if ($synced) { exit 0 } else { exit 1 }
} finally { Pop-Location }
