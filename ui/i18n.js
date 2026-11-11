// UI strings. English lives here and is the fallback; every other language is ui/locales/<code>.js,
// loaded after this file. Tweak names/descriptions come from the catalog (English).
// t("key", {n: 3}) replaces {n}. Missing keys fall back to English, then to the key itself.
const I18N = {
  en: {
    "disc.accept": "I understand",
    "disc.body": "RigSweep changes Windows services, scheduled tasks, registry policies and Microsoft Defender settings, and it can end running processes. Every tweak is backed up and can be restored, and nothing happens without your confirmation, but you use it at your own risk. It is provided \"as is\", without warranty of any kind (MIT license). Review each change before applying it, keep your own backups, and don't use it on work- or school-managed devices without your IT team's approval. RigSweep is an independent project, not affiliated with Microsoft, Google, Anthropic, OpenAI or any other tool it mentions; all trademarks belong to their owners.",
    "disc.title": "⚠️ Disclaimer",
    "clean.vm.warn": "⚠ Shutting down WSL stops every Linux distro and Docker container on this PC. They start again the next time you open WSL or Docker.",
    "kind.vm": "🐧 VM",
"kind.browser": "🌐 automation browser", "reason.mcp": "🔌 MCP server", "reason.duplicate": "👯 older copy (newer: PID {p})",
"live.now": "Live · last 5 min", "live.recovering": "Measuring what this cleanup recovered…", "live.recovered": "↓ {mem} RAM recovered", "live.cpu": "CPU {a}% → {b}%", "live.auto": "auto-cleanup", "live.estimate": "Windows is still reclaiming memory (≈ {mem} expected)",
    "reason.daemon": "♻️ build daemon (restarts when needed)",
    "nav.dashboard": "🏠 Dashboard", "nav.cleanup": "🧹 Dev Cleanup", "nav.report": "📊 Boost Report",
    "nav.tweaks": "⚙️ Tweaks", "nav.processes": "🔍 Processes", "nav.history": "🗂️ History & Backups", "nav.settings": "🛠️ Settings",
    "tagline": "Room for your agents",
    "watchdog.every": "Watchdog: every {n} min", "watchdog.paused": "Watchdog: paused",
    "admin.ok": "Administrator", "admin.bad": "Not elevated!",
    "common.refresh": "Refresh", "common.cancel": "Cancel", "common.ok": "OK", "common.save": "Save", "common.working": "Working…",

    "dash.title": "Dashboard", "dash.cpu": "CPU", "dash.mem": "Memory", "dash.procs": "Processes / running services",
    "dash.tweaks": "Tweaks active", "dash.drift": "Reverted by Windows (fixed)", "dash.top": "Top resource consumers",
    "dash.recent": "Recent activity", "dash.empty": "No activity yet. Start with 🧹 Dev Cleanup or ⚙️ Tweaks.",
    "dash.stale": "🧟 {n} stale dev process tree(s) holding {mem}", "dash.stale.go": "Review in Dev Cleanup →",
    "dash.chrome": "🌐 Chrome: {n} processes using {mem}", "dash.chrome.go": "Get the tab helper →",
    "col.process": "Process", "col.cpu": "CPU", "col.ram": "RAM", "col.disk": "Disk/s", "col.pid": "PID", "col.services": "Hosted services",

    "clean.title": "Dev Cleanup",
    "clean.hint": "Dev servers, build daemons and AI-agent CLIs left behind by closed terminals and old sessions: Node, Python, Java/Gradle, .NET, Ruby, PHP, Rust, Go, Claude Code, Codex, Aider…, the MCP servers and automation browsers (Playwright, Puppeteer) they leave behind, plus the WSL 2 / Docker VM. Each row is a whole process tree. Nothing is ended until you confirm. Processes owned by your editor (language servers) or a desktop app are locked 🔒.",
    "clean.scan": "Rescan", "clean.end": "End selected", "clean.selectStale": "Select stale",
    "clean.summary": "{n} dev process tree(s) using {mem} · {s} stale holding {sm}",
    "clean.none": "No Node / agent CLI processes running. Your rig is clean ✨",
    "clean.noStale": "Nothing is stale right now.", "clean.auto.done": "🧹 Auto-cleanup ended {n} stale tree(s), freed ≈ {mem}.",
    "clean.col.what": "What", "clean.col.project": "Project", "clean.col.age": "Age", "clean.col.mem": "RAM (tree)",
    "clean.col.ports": "Ports", "clean.col.status": "Status",
    "reason.orphan": "🧟 orphan: terminal closed", "reason.idle": "💤 idle", "reason.serving": "🔌 serving",
    "reason.owned": "🔒 owned by {o}", "reason.active": "✅ active",
    "kind.agent": "🤖 agent", "kind.runtime": "⬢ runtime",
    "clean.confirm.title": "End {n} process tree(s)?",
    "clean.confirm.body": "These will be force-closed with all their child processes. Unsaved state in them (an agent's running task, a dev server) is lost.",
    "clean.serving.warn": "⚠ {n} of them are listening on a port, so something may still be using them.",
    "clean.done": "✅ Ended {n} tree(s), freed ≈ {mem}.", "clean.failed": "Some could not be ended:",
    "clean.seeReport": "See the before/after in 📊 Boost Report.",

    "report.title": "Boost Report",
    "report.hint": "Real measurements of this PC, not estimates. A baseline is captured automatically before the first tweak, and before/after each Dev Cleanup. Service tweaks show their full effect after a reboot.",
    "report.measure": "📸 Measure now", "report.copy": "📋 Copy as Markdown", "report.clear": "Clear all",
    "report.before": "Before", "report.after": "After", "report.change": "Change",
    "report.none": "No measurements yet. Click 📸 Measure now to capture a baseline, then clean up and measure again.",
    "report.copied": "Copied! Paste it into an issue, a tweet or your README.",
    "report.label": "Label for this measurement", "report.clear.confirm": "Delete all saved measurements?",
    "m.mem_used": "🧠 RAM in use", "m.mem_free": "🆓 RAM free", "m.cpu_avg": "⚡ CPU (avg)", "m.process_count": "🔢 Processes",
    "m.service_running": "🧩 Running services", "m.chrome_mem": "🌐 Chrome RAM", "m.chrome_procs": "🌐 Chrome processes",
    "m.dev_mem": "⬢ Node / agent RAM", "m.dev_procs": "⬢ Node / agent processes",
    "report.win": "🏆 You freed {mem} of RAM and {p} processes.",
    "label.baseline": "baseline (before first tweak)", "label.before cleanup": "before cleanup", "label.after cleanup": "after cleanup", "label.manual": "manual",

    "tw.title": "Tweaks", "tw.selectSafe": "Select all safe", "tw.applySel": "Apply selected", "tw.verify": "Verify & re-apply profile",
    "tw.hint": "{safe} no noticeable side effects · {mod} small trade-off, read the note · {caution} breaks a feature; you'll be asked to confirm. Every change is backed up first and can be restored.",
    "tw.loading": "Reading current system state…", "tw.attention": "⚠ {n} need attention", "tw.count": "{a}/{n} applied",
    "tw.catApply": "Apply category", "tw.catRestore": "Restore category", "tw.changes": "{n} change(s)",
    "tw.apply": "Apply", "tw.restore": "Restore", "tw.drift": "Times Windows reverted this and RigSweep re-applied it",
    "st.applied": "✓ applied", "st.partial": "partially applied", "st.not_applied": "Windows default", "st.not_present": "not on this PC", "st.error": "error",
    "risk.safe": "safe", "risk.moderate": "moderate", "risk.caution": "⚠ caution",
    "tw.nothingSel": "Nothing selected in this category. Tick the tweaks you want first.", "tw.nothingRestore": "Nothing to restore in this category.",
    "tw.already": "Selected tweaks are already applied.", "tw.sideEffects": "You're about to apply {n} tweak(s). These have side effects:",
    "tw.backedUp": "Original values are backed up and can be restored any time.", "tw.attentionTitle": "⚠ Attention required", "tw.confirm": "Confirm",
    "tw.applying": "Applying…", "tw.applied": "Applied {ok}/{n} tweak(s).", "tw.issues": "Issues:", "tw.restoring": "Restoring…",
    "tw.restored": "Restored {n} tweak(s) to original values.", "tw.restoreFail": "Some restores failed:",
    "tw.verifying": "Verifying…", "tw.checked": "Checked {c} tweak(s): {d} had been reverted, {f} fixed.",
    "tw.removeTitle": "Remove custom tweak", "tw.removeBody": "The service will be restored to its original start type and removed from the list.", "tw.remove": "Remove",

    "proc.title": "Processes", "proc.filter": "Filter by name, PID or service…", "proc.auto": "Auto-refresh",
    "proc.end": "End", "proc.tweak": "tweak", "proc.disable": "disable…", "proc.protected": "Core Windows service, protected",
    "proc.endTitle": "End process?", "proc.endBody": "End <b>{name}</b> (PID {pid})? Unsaved work in it will be lost. It may restart on its own if a service or app relaunches it.",
    "proc.ended": "Ended {name}", "proc.disableTitle": "⚠ Disable unknown service",
    "proc.disableBody": "Disable service <b>{d}</b> (<code>{n}</code>)?", "proc.disableWarn": "⚠ This service isn't in the curated catalog. It will be added under \"Custom\" with a caution flag, its start type backed up, then disabled and stopped. Restore it from the Tweaks tab if anything breaks.",
    "proc.disabled": "Disabled {n}", "proc.problem": "Problem: {m}",

    "hist.title": "History & Backups", "hist.export": "Export profile", "hist.import": "Import profile", "hist.restoreAll": "Restore everything",
    "hist.info": "{n} original value(s) backed up in {dir}\\rigsweep.db (the DB itself is copied to .bak.1-.bak.5 on every start).",
    "hist.col.time": "Time", "hist.col.source": "Source", "hist.col.tweak": "Tweak", "hist.col.action": "Action", "hist.col.result": "Result", "hist.col.detail": "Detail",
    "hist.restoreTitle": "Restore everything?", "hist.restoreBody": "Every change RigSweep made will be reverted to the original values captured before the first change, and your profile will be cleared. The startup task stays as configured in Settings.",
    "hist.restoredErr": "Restored with {n} error(s); see History.", "hist.restored": "Restored {n} tweak(s).",
    "hist.exported": "Profile exported to\n{p}", "hist.importNone": "Profile imported, no tweaks to apply.",
    "hist.importTitle": "Apply imported profile?", "hist.importBody": "The profile contains {n} tweak(s). Review them on the Tweaks tab, or apply now.", "hist.review": "Review & apply",

    "set.title": "Settings", "set.general": "⚙️ General", "set.lang": "🌍 Language",
    "set.startup": "Start with Windows (re-applies your profile at every logon, elevated via Task Scheduler)",
    "set.tray": "Stay in tray after logon (keeps the watchdog running)", "set.watchdog": "Watchdog interval (minutes)",
    "set.idle": "Dev Cleanup: treat a dev process with no CPU activity as idle after (minutes)",
    "set.badge": "Show a dot on the tray / taskbar icon when stale dev processes hold a lot of memory",
    "set.badgeWarn": "🟠 Orange dot from (GB)", "set.badgeAlert": "🔴 Red dot from (GB)",
    "set.autokill": "Dev Cleanup: automatically end stale dev servers once they have been idle that long (checked every minute; agent CLIs, the WSL/Docker VM and editor-owned processes are never touched)",
    "set.folders": "Dev folders excluded from Defender scanning (one per line, %ENV% allowed). Used by the \"Defender exclusions for dev folders\" tweak.",
    "set.saved": "Settings saved.", "set.saving": "Saving…", "set.info": "Startup task: {s} · Data: {d}",
    "set.registered": "registered ✓", "set.notRegistered": "not registered",
    "av.title": "🛡️ Antivirus trust",
    "av.hint": "RigSweep is open source but not code-signed yet, so Microsoft Defender's machine-learning heuristics can flag it by mistake (e.g. \"Trojan:Win32/Bearfoos!ml\") and delete it. If that happened to you, you can exclude only RigSweep's own folder. This is optional and you can undo it any time.",
    "av.folder": "Folder: {f}", "av.on": "✅ Excluded from Defender scanning", "av.off": "Not excluded (Defender scans RigSweep normally)",
    "av.na": "Microsoft Defender isn't the active antivirus here. Add RigSweep's folder to your antivirus' allow list instead.",
    "av.allow": "Allow RigSweep in Defender", "av.disallow": "Remove exclusion",
    "av.confirm": "Exclude only this folder from Microsoft Defender scanning?<br><code>{f}</code><br><br>Do this only if you got RigSweep from the official GitHub releases or built it yourself.",
    "chrome.title": "🌐 Chrome tab helper",
    "chrome.hint": "A desktop app can't close Chrome tabs, so RigSweep ships a tiny extension (no network access, only tabs/storage/alarms permissions): 💤 sleep idle tabs (frees their RAM, tab stays), 🧹 close duplicates, 🎯 close tabs by pattern like localhost:*.",
    "chrome.open": "Open extension folder",
    "chrome.steps": "<ol><li>Open <code>chrome://extensions</code></li><li>Turn on <b>Developer mode</b> (top right)</li><li>Click <b>Load unpacked</b> and pick the folder that just opened</li><li>Pin 🧹 RigSweep Tabs to the toolbar</li></ol>",
    "chrome.opened": "Folder opened: {p}",
    "nav.browser": "🌐 Browser Cleanup",
    "bc.title": "Browser Cleanup",
    "bc.hint": "Empties browser caches per profile: Chrome, Edge, Brave, Vivaldi, Opera and Firefox. Sign-ins, cookies, passwords, history and settings are never touched. Close a browser before cleaning it. Caches refill as you browse, so the big win is usually site data that never shrinks on its own.",
    "bc.scan": "Rescan",
    "bc.scanning": "Measuring…",
    "bc.selectSafe": "Select all safe",
    "bc.clean": "Clean selected",
    "bc.summary": "{safe} in caches · {site} in site data · {sel} selected",
    "bc.none": "No supported browser found.",
    "bc.nothing": "Nothing selected.",
    "bc.running": "open, close it to clean",
    "bc.edgeHint": "Edge often keeps running after you close it: turn off Settings → System and performance → Startup boost and \"Continue running background extensions and apps\".",
    "bc.shared": "All profiles",
    "bc.item.cache": "Pages, images & scripts",
    "bc.item.sw": "Site offline caches",
    "bc.item.gpu": "GPU shader cache",
    "bc.item.filesystem": "Site files (old File System API)",
    "bc.item.indexeddb": "Site databases (IndexedDB)",
    "bc.tier.safe": "safe",
    "bc.tier.site": "site data",
    "bc.col.profile": "Profile",
    "bc.col.what": "What",
    "bc.col.type": "Type",
    "bc.col.size": "Size",
    "bc.confirm.title": "Clean {size}?",
    "bc.confirm.body": "These folders will be emptied. You stay signed in everywhere. Pages load a bit slower the first time while the cache refills.",
    "bc.site.warn": "⚠ Site data is selected: sites lose files they saved for offline use (downloads, offline videos), and a few web apps such as WhatsApp Web or Telegram Web may ask you to sign in again.",
    "bc.done": "✅ Freed {mem}.",
    "bc.locked": "{n} file(s) were in use and were left in place.",
    "bc.skipped": "Skipped because it is open: {b}",
    "clean.procs": "{n} proc",
    "heat.title": "🌡️ Temperature", "heat.cpu": "CPU", "heat.gpu": "GPU",
    "heat.na": "Not reported", "heat.naHint": "This PC doesn't expose temperature sensors to Windows.",
    "heat.peak": "Peak (5 min): {t}", "heat.acpi": "CPU ≈ ACPI zone",
    "heat.unitHint": "Click to switch between °C and °F. The CPU value comes from the motherboard's ACPI thermal zone: close to the CPU on most laptops, approximate on many desktops. The GPU value is the same one Task Manager shows.",
    "heat.hotCallout": "🔥 {what} is running hot ({t}). Stale dev servers and agent processes keep it busy; ending them can help it cool down.",
    "upd.title": "🔄 Updates",
    "upd.hint": "RigSweep only checks for new versions if you ask it to. A check downloads a small version file from GitHub; nothing about your PC is sent. Before installing, the update's signature is verified against RigSweep's own key.",
    "upd.auto": "Check for updates automatically (at startup, then once a day)",
    "upd.check": "Check now",
    "upd.checking": "Checking…",
    "upd.current": "You have version {v}.",
    "upd.latest": "✓ You have the latest version ({v}).",
    "upd.available": "🎉 RigSweep {v} is available (you have {c}).",
    "upd.notes": "What's new",
    "upd.install": "Install and restart",
    "upd.pill": "⬆ Update to {v}",
    "upd.confirm.title": "Install RigSweep {v}?",
    "upd.confirm.body": "RigSweep closes, the installer updates it, and RigSweep starts again. Your tweaks, backups and settings are kept.",
    "upd.downloading": "Downloading… {p}",
    "upd.error": "Couldn't check for updates: {m}",
  },
};

// Native names, in the order the picker shows them.
const LANGS = {
  en: "English", de: "Deutsch", es: "Español", fr: "Français", it: "Italiano", "pt-BR": "Português (Brasil)",
  ru: "Русский", ja: "日本語", ko: "한국어", "zh-CN": "简体中文",
};
// a bare language code maps to the variant we ship (pt-PT → pt-BR, zh-TW → zh-CN is still better than English)
const LANG_ALIAS = { pt: "pt-BR", zh: "zh-CN" };

function resolveLang(tag) {
  if (!tag) return null;
  if (I18N[tag]) return tag;
  const base = tag.split("-")[0].toLowerCase();
  const l = I18N[base] ? base : LANG_ALIAS[base];
  return l && I18N[l] ? l : null;
}

let LANG = "en";

// Called once every locale file has loaded (see index.html).
function initLang() {
  let saved = null;
  try { saved = localStorage.getItem("lang"); } catch {}
  LANG = resolveLang(saved) || (navigator.languages || [navigator.language]).map(resolveLang).find(Boolean) || "en";
}

function t(key, vars) {
  let s = I18N[LANG][key] ?? I18N.en[key] ?? key;
  if (vars) s = s.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? vars[k] : m));
  return s;
}

/** Fill every [data-i18n] / [data-i18n-ph] / [data-i18n-title] element. */
function applyI18n(root = document) {
  document.documentElement.lang = LANG;
  root.querySelectorAll("[data-i18n]").forEach((el) => (el.textContent = t(el.dataset.i18n)));
  root.querySelectorAll("[data-i18n-ph]").forEach((el) => (el.placeholder = t(el.dataset.i18nPh)));
  root.querySelectorAll("[data-i18n-title]").forEach((el) => (el.title = t(el.dataset.i18nTitle)));
}

function setLang(l) {
  l = resolveLang(l);
  if (!l) return;
  LANG = l;
  try { localStorage.setItem("lang", l); } catch {}
  applyI18n();
}
