# RigSweep Tabs

Browser companion of RigSweep, the free MIT-licensed Windows app that frees resources on dev laptops cluttered by AI-agent sessions. RigSweep Tabs frees RAM held by Chrome tabs without losing work.

## Install

1. Open `chrome://extensions`.
2. Turn on **Developer mode** (top right).
3. Click **Load unpacked** and select this `chrome-extension` folder.

## Features

- **Stats**: total tabs, sleeping (discarded) tabs, windows.
- **💤 Sleep idle tabs**: discards background tabs not used for N minutes (default 30), or all background tabs at once. Sleeping tabs stay in the tab strip and reload when clicked. Active, pinned, audible tabs and tabs that opted out of discarding are skipped.
- **🧹 Close duplicate tabs**: same URL (ignoring `#hash`); keeps the active / most recently used copy. Shows the count before closing. Pinned tabs are never closed.
- **🎯 Close tabs matching patterns**: editable list (defaults `localhost:*`, `127.0.0.1:*`), `*` wildcards matched against host + path. Matching tabs are listed with checkboxes for confirmation. Pinned tabs are excluded unless "Include pinned tabs" is ticked.
- **⏰ Auto-sleep**: runs the idle-sleep every 10 minutes in the background.
- Localized in English, Spanish and French.

## Permissions

| Permission | Why |
|---|---|
| `tabs` | Read tab URLs/titles/last-accessed time to find idle, duplicate and matching tabs; discard and close tabs. |
| `storage` | Save your settings (idle minutes, auto-sleep, patterns) via `chrome.storage.sync`. |
| `alarms` | Schedule the 10-minute auto-sleep. |

No host permissions, no content scripts, no remote code.

## Privacy

The extension makes no network requests. No data leaves the browser; settings are stored only in Chrome's own storage (synced by Chrome if you use Chrome Sync).

## License

MIT, part of RigSweep.
