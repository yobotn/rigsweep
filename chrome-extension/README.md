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
- **📈 Counters**: how many tabs it has slept and closed since it was installed.
- Localized in English, Spanish and French.

## Permissions

| Permission | Why |
|---|---|
| `tabs` | Read tab URLs/titles/last-accessed time to find idle, duplicate and matching tabs; discard and close tabs. |
| `storage` | Save your settings (idle minutes, auto-sleep, patterns) via `chrome.storage.sync`. |
| `alarms` | Schedule the 10-minute auto-sleep. |

No host permissions, no content scripts, no remote code. The anonymous weekly stats (below) need no extra permission: `stats.rigsweep.com` allows the request with CORS.

## Privacy

The first time you open the popup, it asks whether to share **anonymous weekly counts** (on by default outside the EU/EEA, UK and Switzerland, where it waits for a yes). Nothing is sent before you answer. A report holds only: tabs slept and closed since the last report, tabs and windows open, whether auto-sleep is on, the browser name and major version, the extension version, and a random install ID made in the browser. Never URLs, page titles, sites, your patterns or anything you browse. *Show what's sent* in the popup displays the exact report, and the switch at the bottom turns sharing off and deletes what was sent. That is the only network request. Settings and counters are stored in Chrome's own storage. Full policy: https://rigsweep.com/privacy/

## License

MIT, part of RigSweep.
