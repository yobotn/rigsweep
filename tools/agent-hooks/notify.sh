#!/usr/bin/env bash
# notify.sh <event> [message]
# event: done | failed | attention
# Opt-in: does nothing unless PES_NOTIFY=1. PES_SOUND=0 disables sound. PES_QUIET_HOURS="22-07" suppresses sound.
# Never blocks: always exits 0, runs in < 1 s, sends nothing off the machine.
event="${1:-done}"; msg="${2:-}"
[ "${PES_NOTIFY:-0}" = 1 ] || exit 0

case "$event" in
  done)      title="Task finished";   urgency=normal ;;
  failed)    title="Task failed";     urgency=critical ;;
  attention) title="Needs your input"; urgency=critical ;;
  *)         title="$event";           urgency=normal ;;
esac
case "$msg" in "{"*) msg="" ;; esac   # Codex appends a JSON payload; don't show it
[ -z "$msg" ] && msg="$(basename "$PWD")"

sound=1
[ "${PES_SOUND:-1}" = 0 ] && sound=0
if [ -n "${PES_QUIET_HOURS:-}" ]; then
  h=$((10#$(date +%H))); s=${PES_QUIET_HOURS%-*}; e=${PES_QUIET_HOURS#*-}; s=$((10#$s)); e=$((10#$e))
  if { [ "$s" -gt "$e" ] && { [ "$h" -ge "$s" ] || [ "$h" -lt "$e" ]; }; } || { [ "$s" -le "$e" ] && [ "$h" -ge "$s" ] && [ "$h" -lt "$e" ]; }; then sound=0; fi
fi

{
  if command -v terminal-notifier >/dev/null 2>&1; then
    terminal-notifier -title "$title" -message "$msg" >/dev/null 2>&1
  elif command -v osascript >/dev/null 2>&1; then
    osascript -e "display notification \"${msg//\"/}\" with title \"${title}\"" >/dev/null 2>&1
  elif command -v notify-send >/dev/null 2>&1; then
    notify-send -u "$urgency" "$title" "$msg" >/dev/null 2>&1
  elif command -v powershell.exe >/dev/null 2>&1; then
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$(dirname "$0")/notify.ps1" -Event "$event" -Message "$msg" >/dev/null 2>&1
    exit 0
  else
    printf '\033]9;%s: %s\007' "$title" "$msg" > /dev/tty 2>/dev/null   # OSC 9 terminal notification
  fi
  [ "$sound" = 1 ] && printf '\a' > /dev/tty 2>/dev/null
} &
exit 0
