# Privacy policy

> RigSweep has no account and no ads. It shares anonymous usage stats only after you answer a first-run question, never with paths or names, and deletes them when you turn sharing off.

*Last updated: November 17, 2026*

**Short version:** RigSweep has no account and no ads. It can share **anonymous usage stats** (what it cleaned, by type and size, and which well-known apps use the most), but only after you answer a question the first time you open it. The report never contains file paths, project names or anything that identifies you, you can see exactly what is sent, and turning sharing off deletes what was sent.

## The app

This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it. RigSweep can make two kinds of network request, each only after you choose it.

## Update check

Only when you ask for it: clicking *Check now* in Settings, or turning on *Check for updates automatically* (off by default). It downloads `latest.json` from the project's GitHub Releases and sends nothing about your PC. An update is installed only after you click *Install*, and only if its signature matches RigSweep's signing key. Like any download, that request is handled by GitHub under [GitHub's privacy statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement).

## Anonymous usage stats

**Why:** to show public counters of how much RigSweep frees for everyone, and to learn which cleanups and which apps matter most, so we know what to build next.

**When you are asked:** the first time you open RigSweep (or the first time after updating to a version that has this), a dialog explains it and offers *Show what's sent*. Nothing is sent until you answer. In most countries the suggested answer is to share; in the EU/EEA, the UK and Switzerland, and wherever the region can't be told, it stays off unless you choose to share. The region is worked out on your PC from the Windows home location and time zone, and is never sent.

**What a report contains**, about once an hour while RigSweep is running:

- a random install ID made on your PC (not derived from your hardware; you can replace it at any time)
- the RigSweep version and the Windows build number
- each cleanup since the previous report, one entry per type: when it happened (UTC, to the minute), the type (for example "node_modules folders", "MCP servers", "npm cache"), the disk space or memory it freed, how many processes or folders it covered, and whether you started it or it ran automatically. The first report also includes the cleanups RigSweep recorded on your PC before you answered. Entries are stored as they arrive, not merged into totals
- the top 5 apps by CPU and by memory, but only from a fixed list of well-known apps such as Chrome, VS Code or Docker Desktop; anything else is left out
- total RAM, average RAM in use, and the number of CPU cores
- which built-in tweaks are on (custom tweaks are left out, because they name your services)

**What it never contains:** file or folder paths, project, folder or process names (apart from the well-known apps above), command lines, user or computer names, hardware or network IDs, or your files. The server rejects any report with free text in it.

**Your IP address** is used only at the moment a report arrives: Cloudflare, which sits in front of the server, tells it the country, and the server uses the address to limit how often one sender can report. Only the two-letter country code is kept. The IP address is never stored or written to logs.

**Where it goes:** to `stats.rigsweep.com`. Traffic passes through Cloudflare (for HTTPS and protection against attacks) to a server we rent from DigitalOcean, and the reports are stored in a DigitalOcean database. Combined totals across all installs may be shown publicly on rigsweep.com. Reports are not sold or shared with anyone, and there are no third-party trackers or ads.

**Your control:** *Settings → Anonymous stats* shows the exact next report, the last one sent and your install ID. Turning sharing off stops it and asks the server to delete everything sent under your install ID; *New random ID* does the same and starts over with a new one.

## What is stored on your PC

Backups of original Windows settings, cleanup history and before/after measurements are stored locally in `%ProgramData%\RigSweep\rigsweep.db`. None of that is uploaded; only the anonymous totals described above are, if you choose to share them. Uninstalling RigSweep and deleting that folder removes it all.

## The Chrome tab helper

The optional extension uses only the `tabs`, `storage` and `alarms` permissions. The first time you open its popup it asks the same question as the app (with the same regional default), and nothing is sent before you answer. If you share, it sends about once an hour while the browser is open, to `stats.rigsweep.com`: how many tabs it slept and closed since the last report, how many tabs and windows are open, whether auto-sleep is on, the browser name and major version, the extension version and its own random install ID. It never sends URLs, page titles, sites, your patterns or anything about what you browse. The switch at the bottom of the popup turns it off and deletes what was sent. Its settings and counters stay in your browser.

## This website

rigsweep.com is a static site served from the same server as the stats, through Cloudflare. It sets no cookies and runs no analytics or ads. The only thing it stores is your language choice, in your browser's local storage. Our server keeps no access logs. The homepage loads the public totals from `stats.rigsweep.com`; that request carries no cookies and is not logged. Cloudflare processes every request to deliver and protect the site, as described in [its privacy policy](https://www.cloudflare.com/privacypolicy/).

## Questions

Open an issue at [github.com/yobotn/rigsweep/issues](https://github.com/yobotn/rigsweep/issues), or see the [contact page](https://rigsweep.com/contact/).
