#!/usr/bin/env bash
# session-status: report the four session states (task, safe-to-close, committed, pushed).
# Usage: session-status.sh [--json] [--exit-code N] [--path DIR]
# Exit: 0 = fully synced, 1 = work not committed or not pushed, 2 = not a git repo.
set -uo pipefail

json=0; task_exit=""; dir="."
while [ $# -gt 0 ]; do
  case "$1" in
    --json) json=1 ;;
    --exit-code) task_exit="${2:-}"; shift ;;
    --path) dir="${2:-.}"; shift ;;
    -h|--help) sed -n '2,4p' "$0"; exit 0 ;;
  esac
  shift
done

cd "$dir" 2>/dev/null || { echo "session-status: cannot cd to $dir" >&2; exit 2; }
if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  [ "$json" = 1 ] && echo '{"git":false}' || echo "Not a git repository: nothing to sync."
  exit 2
fi

repo=$(basename "$(git rev-parse --show-toplevel)")
branch=$(git symbolic-ref --short -q HEAD || echo "(detached)")
dirty=$(git status --porcelain=v1 2>/dev/null | wc -l | tr -d ' ')
untracked=$(git status --porcelain=v1 2>/dev/null | grep -c '^??' || true)
stashes=$(git stash list 2>/dev/null | wc -l | tr -d ' ')
upstream=$(git rev-parse --abbrev-ref --symbolic-full-name '@{u}' 2>/dev/null || echo "")
ahead=0; behind=0
if [ -n "$upstream" ]; then
  read -r behind ahead < <(git rev-list --left-right --count "$upstream...HEAD" 2>/dev/null || echo "0 0")
fi
state=$(git rev-parse --git-dir)
inprogress=""
for f in MERGE_HEAD REBASE_HEAD CHERRY_PICK_HEAD REVERT_HEAD; do [ -e "$state/$f" ] && inprogress="$f"; done
[ -d "$state/rebase-merge" ] || [ -d "$state/rebase-apply" ] && inprogress="REBASE"

# Background jobs: we can only see the *calling* shell's jobs if sourced; report as unknown otherwise.
jobs_info="not checked (run from your shell: jobs)"

synced=1
[ "$dirty" -gt 0 ] && synced=0
[ -z "$upstream" ] && synced=0
[ "$ahead" -gt 0 ] && synced=0
[ -n "$inprogress" ] && synced=0

if [ -n "$inprogress" ]; then
  verdict="ATTENTION: a $inprogress is in progress. Finish or abort it before closing."
elif [ "$dirty" -gt 0 ]; then
  verdict="NOT SAVED: $dirty uncommitted change(s). Next: git add -A && git commit && git push"
elif [ -z "$upstream" ]; then
  verdict="NOT PUSHED: branch has no upstream. Next: git push -u origin $branch"
elif [ "$ahead" -gt 0 ]; then
  verdict="NOT PUSHED: $ahead local commit(s). Next: git push"
else
  verdict="SYNCED: all work is committed and pushed."
fi
[ "$behind" -gt 0 ] && verdict="$verdict (remote has $behind new commit(s): git pull)"

if [ "$json" = 1 ]; then
  printf '{"repo":"%s","branch":"%s","task_exit":%s,"uncommitted":%s,"untracked":%s,"upstream":"%s","ahead":%s,"behind":%s,"stashes":%s,"in_progress":"%s","synced":%s}\n' \
    "$repo" "$branch" "${task_exit:-null}" "$dirty" "$untracked" "$upstream" "$ahead" "$behind" "$stashes" "$inprogress" \
    "$([ $synced = 1 ] && echo true || echo false)"
else
  task="unknown"; [ -n "$task_exit" ] && { [ "$task_exit" = 0 ] && task="finished (exit 0)" || task="FAILED (exit $task_exit)"; }
  echo "Session status: $repo ($branch)"
  echo "  Task           : $task"
  echo "  Working tree   : $([ "$dirty" = 0 ] && echo clean || echo "$dirty uncommitted change(s), $untracked untracked")"
  echo "  Upstream       : ${upstream:-none}"
  echo "  Local commits  : $ahead ahead, $behind behind"
  echo "  Stashes        : $stashes"
  echo "  Background jobs: $jobs_info"
  echo "  Verdict        : $verdict"
fi
[ $synced = 1 ] && exit 0 || exit 1
