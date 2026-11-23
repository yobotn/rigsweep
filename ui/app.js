const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (s) => document.querySelector(s);
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
// In Hebrew / Arabic, keep "1.2 GB" reading left to right inside right-to-left text (LRI … PDI).
const ltr = (s) => (document.documentElement.dir === "rtl" ? `\u2066${s}\u2069` : s);
const fmtBytes = (b) => {
  const neg = b < 0; b = Math.abs(b);
  if (b < 1024) return ltr(`${neg ? "-" : ""}${b} B`);
  const u = ["KB", "MB", "GB", "TB"]; let i = -1;
  do { b /= 1024; i++; } while (b >= 1024 && i < u.length - 1);
  return ltr(`${neg ? "-" : ""}${b.toFixed(b < 10 ? 1 : 0)} ${u[i]}`);
};
// CPU time and run time: seconds stay seconds, longer reads like an age
const fmtDur = (s) => ltr(s < 60 ? `${Math.round(s)}s` : fmtAge(s));
const fmtAge = (s) => s < 3600 ? `${Math.max(1, Math.round(s / 60))}m` : s < 86400 ? `${(s / 3600).toFixed(1)}h` : `${(s / 86400).toFixed(1)}d`;
// Backend timestamps are local "YYYY-MM-DD HH:MM:SS". timeAgo → "5 minutes ago" / "yesterday" in the UI
// language; fullDate → the exact date and time, for tooltips. Anything unparseable is shown as is.
const parseTs = (ts) => { const d = new Date(String(ts ?? "").replace(" ", "T")); return isNaN(d) ? null : d; };
const AGO_UNITS = [["year", 31536000], ["month", 2592000], ["week", 604800], ["day", 86400], ["hour", 3600], ["minute", 60]];
// minSecs: anything more recent reads as that long ago ("1 minute ago" rather than "now")
function timeAgo(ts, minSecs = 0) {
  const d = parseTs(ts);
  if (!d) return ts ?? "";
  const s = Math.min((d - Date.now()) / 1000, -minSecs), rtf = new Intl.RelativeTimeFormat(LANG, { numeric: "auto" });
  const u = AGO_UNITS.find(([, n]) => Math.abs(s) >= n);
  return u ? rtf.format(Math.trunc(s / u[1]), u[0]) : rtf.format(0, "second");
}
const fullDate = (ts) => parseTs(ts)?.toLocaleString(LANG, { dateStyle: "medium", timeStyle: "short" }) ?? (ts ?? "");
// A self-updating "time ago": any element with data-ago="<ts>" is refreshed by the ticker below, so the
// labels keep up without re-rendering their section. data-ago-after is text that follows the time
// (used by <option>s, which can't hold a child element).
const agoHtml = (ts, minSecs = 0) => `<time data-ago="${esc(ts)}"${minSecs ? ` data-ago-min="${minSecs}"` : ""} title="${esc(fullDate(ts))}">${esc(timeAgo(ts, minSecs))}</time>`;
function refreshAgo() {
  document.querySelectorAll("[data-ago]").forEach((el) => {
    const txt = timeAgo(el.dataset.ago, Number(el.dataset.agoMin) || 0) + (el.dataset.agoAfter ?? "");
    if (el.textContent !== txt) el.textContent = txt;
  });
}
setInterval(() => { if (!document.hidden) refreshAgo(); }, 30000);
document.addEventListener("visibilitychange", () => { if (!document.hidden) refreshAgo(); });

let state = null;          // StateView from backend
let selected = new Set();  // tweak ids checked in the UI
let collapsed = new Set(JSON.parse(safeGet("collapsed") || "[]"));
const LIVE_TABS = new Set(["dashboard", "cleanup"]); // pages that draw the live CPU / memory samples
let snapshot = null;
let procTimer = null;
let currentTab = "dashboard", tabGen = 0; // tabGen counts page switches: a slow first read checks it is still current
let dev = null;            // DevReport
let devSel = new Set();    // "pid:start_time"
const devUnticked = new Set(); // stale rows the user un-ticked on purpose: never re-tick those
let snaps = [];            // saved Boost Report measurements
let avState = null;

function safeGet(k) { try { return localStorage.getItem(k); } catch { return null; } }
function safeSet(k, v) { try { localStorage.setItem(k, v); } catch {} }

// ---------- appearance & features (Settings → Appearance & features) ----------
// Mode: Simple hides expert pages and columns (.pro-only). One flat style with Lucide icons. Monitors: a switched-off monitor is never polled, so it costs nothing.
const PREF_DEFAULTS = { mode: "", mon_gpu: true, mon_ai: true, mon_net: true, mon_heat: true, mon_disk_alerts: true, mon_flashes: true };
const prefs = (() => { try { return { ...PREF_DEFAULTS, ...JSON.parse(safeGet("prefs") || "{}") }; } catch { return { ...PREF_DEFAULTS }; } })();
if (!prefs.mode) {
  // new installs start in Simple; someone who already used RigSweep keeps the full app
  prefs.mode = safeGet("lastInGroup") || safeGet("sort") ? "pro" : "simple";
  safeSet("prefs", JSON.stringify(prefs));
}
function applyPrefs() {
  document.documentElement.dataset.mode = prefs.mode;
  document.documentElement.classList.toggle("no-heat", !prefs.mon_heat);
}
applyPrefs();

// ---------- sortable tables ----------
// Every table with th[data-sort] gets one-click sorting: click a header to sort by it, click again
// to flip. Numbers start biggest first, text A→Z (data-sort-first="-1" flips that). The last choice
// per table is remembered, so each tab opens already sorted the way you left it.
const SORT_DEFAULTS = { proc: { key: "cpu", dir: -1 }, dev: { key: "mem", dir: -1 }, disk: { key: "size", dir: -1 }, bc: { key: "size", dir: -1 }, st: { key: "what", dir: 1 } };
const sorts = (() => { try { return { ...SORT_DEFAULTS, ...JSON.parse(safeGet("sort") || "{}") }; } catch { return { ...SORT_DEFAULTS }; } })();
const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });
// Comparator for the table's current sort; get(row, key) returns a number or a string.
function sorter(id, get) {
  const { key, dir } = sorts[id];
  return (a, b) => {
    const va = get(a, key), vb = get(b, key);
    return (typeof va === "string" || typeof vb === "string" ? collator.compare(String(va ?? ""), String(vb ?? "")) : (va ?? -Infinity) - (vb ?? -Infinity)) * dir;
  };
}
function wireSort(tableSel, id, render) {
  const ths = [...document.querySelectorAll(`${tableSel} th[data-sort]`)];
  const paint = () => ths.forEach((th) => {
    const on = th.dataset.sort === sorts[id].key;
    if (on) th.setAttribute("aria-sort", sorts[id].dir > 0 ? "ascending" : "descending"); else th.removeAttribute("aria-sort");
  });
  ths.forEach((th) => {
    // a real button inside the header: reachable by Tab, Enter/Space sorts; the label keeps its i18n key
    const btn = document.createElement("button");
    btn.type = "button"; btn.className = "th-sort";
    const label = document.createElement("span");
    label.dataset.i18n = th.dataset.i18n; label.textContent = t(th.dataset.i18n);
    th.removeAttribute("data-i18n");
    th.textContent = ""; btn.append(label); th.append(btn);
    btn.addEventListener("click", () => {
      const k = th.dataset.sort, first = Number(th.dataset.sortFirst) || (th.classList.contains("num") ? -1 : 1);
      sorts[id] = { key: k, dir: sorts[id].key === k ? -sorts[id].dir : first };
      const mine = Object.fromEntries(Object.entries(sorts).filter(([n, v]) => v.key !== SORT_DEFAULTS[n].key || v.dir !== SORT_DEFAULTS[n].dir));
      safeSet("sort", JSON.stringify(mine));
      paint(); render();
    });
  });
  paint();
}

// ---------- UI helpers ----------
// toast(msg, kind): kind is "ok" | "info" | "warn" | "err" (true = "err", for older call sites).
// The first line reads as the title. Toasts stack, pause while hovered, and close on ×.
const TOAST_ICON = { ok: "✓", info: "i", warn: "!", err: "!" };
function toast(msg, kind = "info", ms) {
  kind = kind === true || kind === "error" ? "err" : kind || "info";
  ms ??= kind === "warn" ? 8000 : 4500;
  const box = $("#toasts");
  // the same message again (a refresh that keeps failing) replaces the old toast instead of piling up
  [...box.children].forEach((x) => { if (x._msg === msg) x.remove(); });
  while (box.children.length >= 4) box.firstElementChild.remove();
  const el = document.createElement("div");
  el.className = `toast ${kind}`;
  el._msg = msg;
  if (kind === "ok") msg = msg.replace(/^✅\s*/u, ""); // the icon already says it
  if (kind === "err") el.setAttribute("role", "alert");
  el.innerHTML = `<span class="t-icon" aria-hidden="true">${TOAST_ICON[kind]}</span><div class="t-msg"></div><button class="t-close" aria-label="${esc(t("common.close"))}">×</button><i class="t-bar"></i>`;
  el.querySelector(".t-msg").textContent = msg;
  const bar = el.querySelector(".t-bar");
  // an error stays until it is dismissed: the reader may need longer than any timer to act on it
  if (kind === "err") bar.remove();
  else bar.style.animationDuration = `${ms}ms`;
  const close = () => {
    if (el._closing) return;
    el._closing = true;
    el.classList.add("out");
    setTimeout(() => el.remove(), 180);
  };
  bar.addEventListener("animationend", close);
  el.querySelector(".t-close").onclick = close;
  box.appendChild(el);
}

let modalDone = null; // the open dialog's close function: a dialog opened from inside another replaces it cleanly
function modal(title, html, { okText = t("common.ok"), cancelText = t("common.cancel"), danger = false, cancel = true } = {}) {
  const outer = modalDone && modalDone(false, true);
  return new Promise((resolve) => {
    const box = $("#modal"), ok = $("#modal-ok"), cn = $("#modal-cancel");
    const prev = outer || document.activeElement;
    $("#modal-title").textContent = title;
    $("#modal-body").innerHTML = html;
    ok.textContent = okText;
    cn.textContent = cancelText;
    ok.className = danger ? "danger" : "primary";
    cn.style.display = cancel ? "" : "none";
    box.classList.remove("hidden");
    // the page behind is inert while the dialog is open: Tab stays in the dialog, screen readers read only it
    const behind = [document.querySelector(".sidebar"), document.querySelector("main")];
    behind.forEach((el) => { if (el) el.inert = true; });
    // Esc and a click outside cancel; a destructive dialog starts on Cancel, not on the red button
    const onKey = (e) => { if (e.key === "Escape" && cancel) { e.preventDefault(); done(false); } };
    const done = (v, replaced) => {
      modalDone = null;
      document.removeEventListener("keydown", onKey, true);
      resolve(v);
      if (replaced) return prev; // the next dialog takes over the box; focus goes back where this one came from
      box.classList.add("hidden");
      behind.forEach((el) => { if (el) el.inert = false; });
      ok.onclick = cn.onclick = box.onclick = null;
      // the button that opened the dialog may be gone (its table was redrawn): fall back to the page's tab or nav item
      (prev?.isConnected ? prev : document.querySelector(".tab.active .pagetabs .active, nav button.active"))?.focus?.();
    };
    modalDone = done;
    ok.onclick = () => done(true);
    cn.onclick = () => done(false);
    box.onclick = (e) => { if (e.target === box && cancel) done(false); };
    document.addEventListener("keydown", onKey, true);
    (danger && cancel ? cn : ok).focus();
  });
}

// call() shows a failure as a toast; { quiet: true } leaves it to the caller (an error row in place, say)
async function call(cmd, args, { quiet = false } = {}) {
  try { return await invoke(cmd, args); }
  catch (e) { console.error(cmd, e); if (!quiet) toast(`${t("err.failed")}\n${e}`, "err"); throw e; }
}

// redraw a live table without dropping keyboard focus: the focused button is found again by its first data-* value
function keepFocus(box, draw) {
  const a = document.activeElement, d = a && box.contains(a) && Object.entries(a.dataset || {})[0];
  draw();
  if (!d || box.contains(document.activeElement)) return;
  const attr = "data-" + d[0].replace(/[A-Z]/g, (c) => "-" + c.toLowerCase());
  box.querySelector(`[${attr}="${CSS.escape(d[1])}"]`)?.focus();
}

function busy(btn, on, label) {
  if (!btn) return;
  // the button keeps its width while it says "Working…", so the toolbar around it doesn't jump
  if (on) { btn._t = btn.textContent; btn.style.minWidth = `${btn.offsetWidth}px`; btn.disabled = true; btn.classList.add("is-busy"); btn.setAttribute("aria-busy", "true"); btn.textContent = label || t("common.working"); }
  else { btn.disabled = false; btn.classList.remove("is-busy"); btn.removeAttribute("aria-busy"); btn.style.minWidth = ""; if (btn._t) btn.textContent = btn._t; }
}

// ---------- keeping scanned lists current ----------
// Dev Cleanup, Browser, Startup and Dev Disk show a scan of things that change outside the app
// (a dev server starts, a browser closes, a build fills target\). Callers share the scan in flight;
// `fresh` asks for one that starts after now (the Scan button, or right after an action).
const scans = {}, scannedAt = {};
function scanOnce(key, fn, fresh = false) {
  const s = scans[key];
  if (s) return fresh ? (s.again ||= s.p.catch(() => {}).then(() => scanOnce(key, fn))) : s.p;
  const p = fn().finally(() => { scannedAt[key] = Date.now(); if (scans[key]?.p === p) delete scans[key]; });
  scans[key] = { p };
  return p;
}
// A list is rescanned when its tab opens or the window comes back once it's older than `stale`,
// and on a timer while it's in view (`every`). Dev Disk walks whole drives, so it has no timer.
const FRESH = {
  cleanup: { stale: 5000, every: 15000, run: () => loadDev() },
  browser: { stale: 15000, every: 60000, run: () => loadBrowsers() },
  startup: { stale: 30000, every: 0, run: () => loadStartup() },
  disk: { stale: 300000, every: 0, run: () => loadDisk(), skip: () => jobActive(diskJob) },
  tweaks: { stale: 60000, every: 0, run: () => scanOnce("tweaks", loadState) }, // status can drift outside the app
};
function refreshTab(tab, why) { // why: "open" | "focus" | "timer"
  const f = FRESH[tab], sec = document.getElementById(`tab-${tab}`);
  if (!f || !sec || document.visibilityState !== "visible" || f.skip?.()) return;
  if (why === "timer" && !f.every) return;
  if (Date.now() - (scannedAt[tab] || 0) < (why === "timer" ? f.every : f.stale)) return;
  // never redraw under a confirm dialog, or while a scan or clean-up on this tab is running
  if (why !== "open" && !$("#modal").classList.contains("hidden")) return;
  if (sec.querySelector("[aria-busy]")) return;
  // the table is redrawn: put keyboard focus back on the same checkbox or button
  const a = document.activeElement, d = a && sec.contains(a) && Object.entries(a.dataset || {})[0];
  Promise.resolve(f.run()).then(() => {
    if (!d || document.activeElement !== document.body) return;
    const attr = "data-" + d[0].replace(/[A-Z]/g, (c) => "-" + c.toLowerCase());
    sec.querySelector(`[${attr}="${CSS.escape(d[1])}"]`)?.focus();
  }).catch(() => {});
}
addEventListener("focus", () => refreshTab(currentTab, "focus"));
document.addEventListener("visibilitychange", () => refreshTab(currentTab, "focus"));
setInterval(() => refreshTab(currentTab, "timer"), 5000);

// label for a history row: translated action and tweak name, raw values as the fallback
const tOr = (k, fb) => { const v = t(k); return v === k ? fb : v; };
const actLabel = (h) => tOr("act." + h.action, h.action);
const tweakName = (id) => (id && state?.tweaks.find((x) => x.id === id)?.name) || id || "";

// ---------- language ----------
initLang();
const langSel = $("#lang-select");
langSel.innerHTML = Object.entries(LANGS).filter(([k]) => I18N[k]).map(([k, v]) => `<option value="${k}">${v}</option>`).join("");
langSel.value = LANG;
langSel.onchange = () => { setLang(langSel.value); rerender(); };

// open Tweaks on one tweak: its category stays open even when the list is redrawn by the rescan the tab switch starts
async function jumpToTweak(id) {
  showTab("tweaks");
  const cat = state?.tweaks?.find((x) => x.id === id)?.category
    || document.querySelector(`.tw[data-id="${CSS.escape(id)}"]`)?.closest(".cat")?.dataset.cat;
  if (cat && collapsed.delete(cat)) { safeSet("collapsed", JSON.stringify([...collapsed])); renderTweaks(); }
  await scans.tweaks?.p?.catch(() => {});
  const node = document.querySelector(`.tw[data-id="${CSS.escape(id)}"]`);
  if (!node) return;
  node.closest(".cat")?.classList.remove("collapsed");
  node.scrollIntoView({ block: "center" });
  node.style.outline = "2px solid var(--accent)";
  setTimeout(() => (node.style.outline = ""), 2500);
}

function rerender() {
  applyI18n();
  $("#tw-hint").innerHTML = t("tw.hint", {
    safe: `<span class="risk safe">${esc(t("risk.safe"))}</span>`,
    mod: `<span class="risk moderate">${esc(t("risk.moderate"))}</span>`,
    caution: `<span class="risk caution">${esc(t("risk.caution"))}</span>`,
  });
  if (state) { setStatusPills(); renderTweaks(); }
  setupIntros();
  renderPageTabs();
  queueMicrotask(renderReady); // after the script has loaded (it reads later bindings)
  renderImpact();
  $("#tab-settings .subtabs")?.setAttribute("aria-label", noIcon(t("nav.settings")));
  if (dev) renderDev();
  if (disk) renderDisk();
  if (bc) renderBrowsers();
  renderUpdate();
  showTab(currentTab);
}

// ---------- "⋯" menus: secondary and dangerous actions ----------
function closeMenus(except) {
  document.querySelectorAll(".menu-list:not([hidden])").forEach((l) => {
    if (l === except) return;
    l.hidden = true;
    l.parentElement.querySelector(".menu-btn").setAttribute("aria-expanded", "false");
  });
}
document.querySelectorAll(".menu").forEach((m) => {
  const btn = m.querySelector(".menu-btn"), list = m.querySelector(".menu-list");
  const items = () => [...list.querySelectorAll("[role=menuitem]")];
  btn.addEventListener("click", (e) => {
    e.stopPropagation();
    const open = list.hidden;
    closeMenus(list);
    list.hidden = !open;
    btn.setAttribute("aria-expanded", String(open));
    if (open) items()[0]?.focus();
  });
  list.addEventListener("click", (e) => { if (e.target.closest("[role=menuitem]")) closeMenus(); });
  list.addEventListener("keydown", (e) => {
    const all = items(), i = all.indexOf(document.activeElement);
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      all[(i + (e.key === "ArrowDown" ? 1 : -1) + all.length) % all.length]?.focus();
    } else if (e.key === "Escape") { closeMenus(); btn.focus(); }
    else if ((e.key === "Enter" || e.key === " ") && document.activeElement.tagName === "LABEL") { e.preventDefault(); document.activeElement.click(); }
  });
});
document.addEventListener("click", (e) => { if (!e.target.closest(".menu")) closeMenus(); });
document.addEventListener("keydown", (e) => { if (e.key === "Escape") closeMenus(); });

// Desktop-app keys. The window is a web view, so F5 / Ctrl+R would reload the whole UI and lose
// selections and running jobs: here they rescan the page instead, like Task Manager or Explorer.
// Ctrl+F goes to the page's filter box; print / save-page / view-source do nothing in an app.
document.addEventListener("keydown", (e) => {
  const ctrl = e.ctrlKey || e.metaKey, k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  if (k === "F5" || (ctrl && k === "r")) {
    e.preventDefault();
    if (e.repeat || !$("#modal").classList.contains("hidden")) return;
    const tab = document.getElementById(`tab-${currentTab}`);
    const b = tab?.querySelector('header [id^="btn-"][id*="refresh"], header [id^="btn-"][id*="scan"]');
    if (b) { if (!b.disabled) b.click(); }
    else FRESH[currentTab]?.run();
    return;
  }
  if (ctrl && k === "f") {
    const f = document.querySelector(`#tab-${currentTab} input[type="search"]:not([hidden])`);
    if (f) { e.preventDefault(); f.focus(); f.select(); }
    return;
  }
  if (ctrl && ["p", "s", "u"].includes(k)) e.preventDefault();
});
// the browser's right-click menu (Back, Reload, Save as, Print…) means nothing here; text fields and
// selected text keep it for copy and paste
document.addEventListener("contextmenu", (e) => {
  if (e.target.closest?.("input, textarea, [contenteditable]") || String(getSelection()).trim()) return;
  e.preventDefault();
});

// ---------- page intros ----------
// Each page's explanation shows one line; "How it works" opens the rest (remembered per page).
const introsOpen = new Set(JSON.parse(safeGet("introsOpen") || "[]"));
function setupIntros() {
  document.querySelectorAll(".intro").forEach((box) => {
    const page = box.closest(".tab")?.id || "";
    let btn = box.querySelector(".intro-toggle");
    if (!btn) {
      btn = document.createElement("button");
      btn.className = "intro-toggle";
      btn.type = "button";
      box.append(btn);
      btn.onclick = () => {
        if (introsOpen.has(page)) introsOpen.delete(page); else introsOpen.add(page);
        safeSet("introsOpen", JSON.stringify([...introsOpen]));
        setupIntros();
      };
    }
    const open = introsOpen.has(page);
    box.classList.toggle("collapsed", !open);
    btn.textContent = open ? t("intro.less") : t("intro.more");
    btn.setAttribute("aria-expanded", open ? "true" : "false");
  });
}
setupIntros();

// ---------- pages and their in-page tabs ----------
// The sidebar has one entry per group; the pages of a group are tabs at the top of the content.
const ALL_GROUPS = { home: ["dashboard", "impact"], clean: ["cleanup", "disk", "browser", "finders"], tune: ["tweaks", "processes", "startup", "flashes"], pc: ["hardware", "gpu", "network", "report"] };
// pages only Pro shows, and pages that belong to a monitor someone switched off
const PRO_PAGES = new Set(["processes", "network"]);
const pageOn = (p) => !(prefs.mode === "simple" && PRO_PAGES.has(p)) && !(p === "gpu" && !prefs.mon_gpu && !prefs.mon_ai) && !(p === "network" && !prefs.mon_net) && !(p === "flashes" && !prefs.mon_flashes);
const visibleGroups = () => Object.fromEntries(Object.entries(ALL_GROUPS).map(([g, ps]) => [g, ps.filter(pageOn)]));
let GROUPS = visibleGroups();
const groupOf = (name) => Object.keys(GROUPS).find((g) => GROUPS[g].includes(name));
const lastInGroup = JSON.parse(safeGet("lastInGroup") || "{}");
const noIcon = (s) => s.replace(/^\P{L}+/u, ""); // drops the leading icon: "Dev Cleanup" from the sidebar label
function renderPageTabs() {
  for (const [g, pages] of Object.entries(GROUPS)) {
    for (const name of pages) {
      const sec = document.getElementById(`tab-${name}`);
      if (!sec) continue;
      sec.classList.add("grouped");
      let strip = sec.querySelector(":scope > .pagetabs");
      if (!strip) {
        strip = document.createElement("div");
        strip.className = "subtabs pagetabs";
        strip.setAttribute("role", "tablist");
        sec.prepend(strip);
      }
      strip.setAttribute("aria-label", noIcon(t("nav." + (g === "home" ? "dashboard" : g))));
      strip.innerHTML = pages.map((p) => `<button role="tab" data-page="${p}" aria-selected="${p === name}" tabindex="${p === name ? 0 : -1}"
        class="${p === name ? "active" : ""}">${esc(noIcon(t("nav." + (p === "dashboard" ? "overview" : p))))}</button>`).join("");
    }
  }
}
document.addEventListener("click", (e) => { const b = e.target.closest(".pagetabs [data-page]"); if (b) showTab(b.dataset.page); });
document.addEventListener("keydown", (e) => {
  const strip = e.target.closest?.(".pagetabs");
  if (!strip || !["ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key)) return;
  const tabs = [...strip.querySelectorAll("[data-page]")], i = tabs.indexOf(e.target), rtl = document.documentElement.dir === "rtl";
  const step = { ArrowRight: rtl ? -1 : 1, ArrowLeft: rtl ? 1 : -1 }[e.key];
  const next = e.key === "Home" ? 0 : e.key === "End" ? tabs.length - 1 : (i + step + tabs.length) % tabs.length;
  e.preventDefault();
  showTab(tabs[next].dataset.page);
  document.querySelector(`#tab-${tabs[next].dataset.page} .pagetabs [data-page="${tabs[next].dataset.page}"]`)?.focus();
});
renderPageTabs();
document.querySelectorAll("nav button").forEach((b) =>
  b.addEventListener("click", () => showTab(b.dataset.group ? (lastInGroup[b.dataset.group] || b.dataset.tab) : b.dataset.tab)));

function showTab(name) {
  if (!document.getElementById(`tab-${name}`)) name = "dashboard";
  if (!pageOn(name)) name = Object.entries(ALL_GROUPS).find(([, ps]) => ps.includes(name))?.[1].find(pageOn) || "dashboard";
  currentTab = name;
  tabGen++;
  const group = groupOf(name);
  if (group) { lastInGroup[group] = name; safeSet("lastInGroup", JSON.stringify(lastInGroup)); }
  document.querySelectorAll("nav button").forEach((b) => {
    const on = b.dataset.group ? b.dataset.group === group : b.dataset.tab === name;
    b.classList.toggle("active", on);
    if (on) b.setAttribute("aria-current", "page"); else b.removeAttribute("aria-current");
  });
  document.querySelectorAll(".tab").forEach((s) => s.classList.toggle("active", s.id === `tab-${name}`));
  // the window title names the page (taskbar, Alt+Tab, screen readers)
  const pageName = noIcon(t("nav." + name));
  document.title = pageName && !pageName.startsWith("nav.") ? `${pageName} · RigSweep` : "RigSweep";
  safeSet("tab", name);
  clearInterval(procTimer);
  if (name === "startup" && startup) renderStartup();
  if (name === "processes") { loadProcesses(); procTimer = setInterval(() => $("#proc-auto").checked && document.visibilityState === "visible" && loadProcesses(), liveMs()); }
  if (LIVE_TABS.has(name) && !document.hidden) queueMicrotask(renderLive); // after the script has loaded (renderLive uses later bindings)
  if (name === "dashboard") { loadDashboard(); procTimer = setInterval(() => document.visibilityState === "visible" && loadDashboard(), liveMs() * 2); }
  if (name === "disk") renderDiskAuto();
  refreshTab(name, "open"); // cleanup, browser, startup, disk: rescan if the last scan is old
  if (name === "report") loadSnapshots();
  if (name === "impact") loadImpact();
  if (name === "hardware") loadHardware();
  if (name === "gpu") openGpu();
  if (name === "network") openNetwork();
  if (name === "finders") openFinders();
  if (name === "flashes") openFlashes();
  if (name === "history") loadHistory();
  if (name === "settings") renderSettings();
}

// ---------- state ----------
async function loadState() {
  state = await call("get_state");
  if (selected.size === 0) {
    // default selection: whatever is already in the profile, otherwise all safe tweaks
    const prof = state.tweaks.filter((x) => x.in_profile).map((x) => x.id);
    selected = new Set(prof.length ? prof : state.tweaks.filter((x) => x.risk === "safe").map((x) => x.id));
  }
  setStatusPills();
  renderTweaks();
}

// Status shows up only when something needs attention: not elevated, or the watchdog paused.
function setStatusPills() {
  const banner = $("#admin-banner");
  banner.textContent = state.elevated ? "" : t("admin.bad");
  banner.classList.toggle("hidden", !!state.elevated);
  setPaused(state.paused);
}

function setPaused(p) {
  if (state) state.paused = p;
  const pill = $("#watchdog-status");
  pill.textContent = p ? t("watchdog.paused") : "";
  pill.classList.toggle("hidden", !p);
}

// ---------- tweaks ----------
function renderTweaks() {
  const list = $("#tweak-list");
  const byCat = {};
  // PC maker services are listed only when that maker's software is on this PC
  state.tweaks.filter((x) => !(x.category === "vendor" && x.status.overall === "not_present")).forEach((x) => (byCat[x.category] ||= []).push(x));
  list.innerHTML = state.categories.filter((c) => byCat[c.id]?.length).map((c) => {
    const items = byCat[c.id];
    const applied = items.filter((x) => x.status.overall === "applied").length;
    const caution = items.filter((x) => x.risk === "caution").length;
    return `
    <div class="cat ${collapsed.has(c.id) ? "collapsed" : ""}" data-cat="${esc(c.id)}">
      <div class="cat-head">
        <span class="caret">▾</span>
        <h2>${esc(c.name)}</h2>
        ${caution ? `<span class="risk caution">${esc(t("tw.attention", { n: caution }))}</span>` : ""}
        <span class="count">${esc(t("tw.count", { a: applied, n: items.length }))}</span>
        <button class="small ghost" data-cat-apply="${esc(c.id)}">${esc(t("tw.catApply"))}</button>
        <button class="small ghost" data-cat-restore="${esc(c.id)}">${esc(t("tw.catRestore"))}</button>
      </div>
      <div class="cat-body">
        <div class="cat-desc">${esc(c.description)}</div>
        ${items.map(renderTweak).join("")}
      </div>
    </div>`;
  }).join("");
}

function renderTweak(x) {
  const st = x.status.overall;
  const warnCls = x.risk === "caution" ? "warn" : "warn mod";
  const actions = x.status.actions.map((a) => {
    const s = a.status.state;
    const d = a.status.detail ? ` — ${esc(a.status.detail)}` : "";
    return `<li>${s === "applied" ? "✓" : s === "not_present" ? "–" : s === "error" ? "✗" : "○"} ${esc(a.target)}${d}</li>`;
  }).join("");
  return `
  <div class="tw" data-id="${esc(x.id)}">
    <input type="checkbox" class="sel" ${selected.has(x.id) ? "checked" : ""} ${st === "not_present" ? "disabled" : ""} />
    <div>
      <div class="name">${esc(x.name)} <details class="chg"><summary>${esc(t("tw.changes", { n: x.status.actions.length }))}</summary><ul>${actions}</ul></details></div>
      <div class="desc">${esc(x.description)}</div>
      ${x.warning ? `<div class="${warnCls}">${x.risk === "caution" ? "⚠ " : ""}${esc(x.warning)}</div>` : ""}
    </div>
    <div class="right">
      <span class="risk ${x.risk}">${esc(t("risk." + x.risk))}</span>
      <span class="st ${st}">${esc(t("st." + st))}</span>
      ${x.drift_count ? `<span class="st drift" title="${esc(t("tw.drift"))}">↻ ${x.drift_count}</span>` : ""}
      ${st !== "applied" && st !== "not_present" ? `<button class="small" data-apply="${esc(x.id)}">${esc(t("tw.apply"))}</button>` : ""}
      ${x.in_profile || st === "applied" || st === "partial" ? `<button class="small ghost" data-restore="${esc(x.id)}">${esc(t("tw.restore"))}</button>` : ""}
      ${x.custom ? `<button class="small ghost" data-remove="${esc(x.id)}">✕</button>` : ""}
    </div>
  </div>`;
}

$("#tweak-list").addEventListener("click", async (e) => {
  const el = e.target;
  const head = el.closest(".cat-head");
  if (el.matches("input.sel")) {
    const id = el.closest(".tw").dataset.id;
    el.checked ? selected.add(id) : selected.delete(id);
    return;
  }
  if (el.dataset.apply) return applyIds([el.dataset.apply], el);
  if (el.dataset.restore) return restoreIds([el.dataset.restore], el);
  if (el.dataset.remove) {
    if (!(await modal(t("tw.removeTitle"), esc(t("tw.removeBody")), { okText: t("tw.remove") }))) return;
    await call("remove_custom", { id: el.dataset.remove });
    selected.delete(el.dataset.remove);
    return loadState();
  }
  if (el.dataset.catApply) {
    const ids = state.tweaks.filter((x) => x.category === el.dataset.catApply && selected.has(x.id)).map((x) => x.id);
    if (!ids.length) return toast(t("tw.nothingSel"));
    return applyIds(ids, el);
  }
  if (el.dataset.catRestore) {
    const ids = state.tweaks.filter((x) => x.category === el.dataset.catRestore && (x.in_profile || x.status.overall !== "not_applied")).map((x) => x.id);
    if (!ids.length) return toast(t("tw.nothingRestore"));
    return restoreIds(ids, el);
  }
  if (head && el.tagName !== "BUTTON") {
    const cat = head.parentElement;
    cat.classList.toggle("collapsed");
    cat.classList.contains("collapsed") ? collapsed.add(cat.dataset.cat) : collapsed.delete(cat.dataset.cat);
    safeSet("collapsed", JSON.stringify([...collapsed]));
  }
});

async function applyIds(ids, btn) {
  const tweaks = ids.map((id) => state.tweaks.find((x) => x.id === id)).filter(Boolean)
    .filter((x) => x.status.overall !== "applied" && x.status.overall !== "not_present");
  if (!tweaks.length) return toast(t("tw.already"));
  const risky = tweaks.filter((x) => x.risk !== "safe");
  const caution = tweaks.filter((x) => x.risk === "caution");
  if (risky.length) {
    const html = `<p>${esc(t("tw.sideEffects", { n: tweaks.length }))}</p><ul>${risky.map((x) =>
      `<li><span class="risk ${x.risk}">${esc(t("risk." + x.risk))}</span> <b>${esc(x.name)}</b><br><span class="sub">${esc(x.warning || x.description)}</span></li>`).join("")}</ul>
      <p class="sub">${esc(t("tw.backedUp"))}</p>`;
    if (!(await modal(caution.length ? t("tw.attentionTitle") : t("tw.confirm"), html, { okText: t("tw.apply"), danger: caution.length > 0 }))) return;
  }
  busy(btn, true, t("tw.applying"));
  try {
    const r = await call("apply_tweaks", { ids: tweaks.map((x) => x.id) });
    const failed = r.results.filter((x) => !x.ok);
    let msg = t("tw.applied", { ok: r.results.length - failed.length, n: r.results.length });
    if (r.restore_point) msg += `\n${r.restore_point}`;
    if (failed.length) msg += `\n\n${t("tw.issues")}\n` + failed.map((f) => `• ${f.tweak_id}: ${f.messages.filter((m) => !m.startsWith("applied")).join("; ")}`).join("\n");
    toast(msg, failed.length ? "warn" : "ok", failed.length ? 15000 : 5000);
  } finally { busy(btn, false); }
  await loadState();
}

async function restoreIds(ids, btn) {
  busy(btn, true, t("tw.restoring"));
  try {
    const r = await call("restore_tweaks", { ids });
    const failed = r.filter((x) => !x.ok);
    toast(failed.length ? `${t("tw.restoreFail")}\n` + failed.map((f) => `• ${f.tweak_id}: ${f.messages.join("; ")}`).join("\n")
      : t("tw.restored", { n: r.length }), failed.length ? "warn" : "ok", failed.length ? 12000 : 4000);
  } finally { busy(btn, false); }
  await loadState();
}

$("#btn-apply-selected").onclick = (e) => selected.size ? applyIds([...selected], e.target.closest("button")) : toast(t("disk.nothing"));
$("#btn-select-safe").onclick = () => {
  state.tweaks.filter((x) => x.risk === "safe" && x.status.overall !== "not_present").forEach((x) => selected.add(x.id));
  renderTweaks();
};
$("#btn-reapply").onclick = async (e) => {
  busy(e.target.closest("button"), true, t("tw.verifying"));
  try {
    const s = await call("reapply_now");
    toast(t("tw.checked", { c: s.checked, d: s.drifted, f: s.fixed }) + (s.errors.length ? `\n${s.errors.join("\n")}` : ""), s.errors.length ? "warn" : "ok");
  } finally { busy(e.target.closest("button"), false); }
  loadState();
};

// ---------- dev cleanup ----------
const devKey = (p) => `${p.pid}:${p.start_time}`;
// ecosystem names are proper nouns, so only "agent" / "VM" are translated
const ECO = { node: "⬢ Node", python: "🐍 Python", java: "☕ Java", dotnet: "🟣 .NET", ruby: "💎 Ruby",
  php: "🐘 PHP", rust: "🦀 Rust", go: "🐹 Go" };
const ecoLabel = (p) => p.kind === "agent" ? t("kind.agent") : p.kind === "vm" ? t("kind.vm")
  : p.eco === "browser" ? t("kind.browser") : p.eco === "wsl" ? t("kind.wsl") : p.eco === "shell" ? t("kind.shell")
  : (ECO[p.eco] || p.eco);
// a first scan that fails says so in place of the spinner, with a button to try again
const scanFailed = {};
function failRow(sel, cols, key, e) {
  scanFailed[key] = true;
  $(sel).innerHTML = `<tr><td colspan="${cols}" class="loading"><p>${esc(t("err.failed"))}: ${esc(e)}</p><button class="ghost" data-rescan="${key}">${esc(t("clean.scan"))}</button></td></tr>`;
  if (typeof renderReady === "function") renderReady();
}
document.addEventListener("click", (e) => {
  const b = e.target.closest("[data-rescan]");
  if (!b) return;
  const key = b.dataset.rescan;
  ({ browser: () => loadBrowsers(b), disk: () => loadDisk(true), startup: () => loadStartup(b) })[key]?.().catch(() => {});
});

// a shell-leftovers row has no folder: show which commands are stuck instead
const devWhere = (p) => p.eco === "shell" ? p.cmd : p.project;

// a button press or an action asks for a fresh scan; a tab switch or timer shares one in flight
async function loadDev(btn, fresh = !!btn) {
  busy(btn, true);
  try { await scanOnce("cleanup", scanDev, fresh); } catch {} finally { busy(btn, false); }
}
async function scanDev() {
  try { dev = await call("get_dev_processes", undefined, { quiet: !dev }); }
  catch (e) {
    // keep the last good list if there is one; otherwise say what failed in place of the spinner
    if (!dev) $("#clean-body").innerHTML = `<tr><td colspan="9" class="loading"><p>${esc(t("err.failed"))}: ${esc(e)}</p><button class="ghost" data-retry>${esc(t("clean.scan"))}</button></td></tr>`;
    throw e;
  }
  // keep the user's ticks across rescans, and tick every stale row, including ones that only
  // became stale after a later scan (idle is measured over time), unless the user un-ticked it
  const keys = new Set(dev.items.map(devKey));
  devSel = new Set([...devSel].filter((k) => keys.has(k)));
  [...devUnticked].forEach((k) => { if (!keys.has(k)) devUnticked.delete(k); });
  dev.items.forEach((p) => { if (p.stale && !p.protected && !devUnticked.has(devKey(p))) devSel.add(devKey(p)); });
  renderDev();
}

function renderDev() {
  if (!dev) return;
  $("#clean-summary").textContent = dev.items.length
    ? t("clean.summary", { n: dev.items.length, mem: fmtBytes(dev.total_mem), s: dev.stale_count, sm: fmtBytes(dev.stale_mem) }) : "";
  if (!dev.items.length) {
    $("#clean-body").innerHTML = `<tr><td colspan="9" class="loading">${esc(t("clean.none"))}</td></tr>`;
    return;
  }
  const devRank = (p) => p.protected ? 0 : p.stale ? 2 + p.reasons.length : 1; // stale with most reasons first
  const devVal = (p, k) => k === "what" ? p.label : k === "project" ? devWhere(p) || "" : k === "age" ? p.age_secs
    : k === "ports" ? p.ports.length : k === "status" ? devRank(p) * 1e15 + p.tree_mem : k === "cpu" ? p.cpu : p.tree_mem;
  $("#clean-body").innerHTML = [...dev.items].sort(sorter("dev", devVal)).map((p) => {
    const k = devKey(p);
    const reasons = p.reasons.length ? p.reasons : ["active"];
    const tip = (r) => r === "orphan" ? ` title="${esc(t("reason.orphanTip"))}"` : r === "managed" ? ` title="${esc(t("reason.managedTip", { m: p.manager }))}"` : "";
    const badges = reasons.map((r) => `<span class="badge ${r}"${tip(r)}>${esc(t("reason." + r, { o: p.owner, p: p.dup_of, m: p.manager }))}</span>`).join(" ");
    // a deliberately detached job (start /b, Start-Process, nohup) can be remembered by its command line
    const keep = !p.protected && p.cmd && (p.kept || p.reasons.includes("orphan") || p.stale)
      ? ` <button class="small ghost" data-keep="${p.kept ? 0 : 1}" data-cmd="${esc(p.cmd)}" title="${esc(t("clean.keepTip"))}">${esc(t(p.kept ? "clean.unkeep" : "clean.keep"))}</button>` : "";
    return `<tr class="${p.stale ? "stale" : ""} ${p.protected ? "locked" : ""}">
      <td>${p.protected ? "🔒" : `<input type="checkbox" data-dev="${esc(k)}" aria-label="${esc(`${p.label} · ${devWhere(p) || p.pid}`)}" ${devSel.has(k) ? "checked" : ""} />`}</td>
      <td><b>${esc(p.label)}</b> <span class="eco ${esc(p.eco)}">${esc(ecoLabel(p))}</span>
        <div class="sub" title="${esc(p.cmd)}">PID ${p.pid} · ${esc(p.parent_name || "—")} · ${esc(t("clean.procs", { n: p.tree_count }))}</div></td>
      <td title="${esc(p.eco === "shell" ? p.cmd : p.project_path)}" class="${p.eco === "shell" || p.eco === "wsl" ? "mono-cut" : ""}"><bdi dir="ltr">${esc(devWhere(p) || "—")}</bdi></td>
      <td class="num">${p.age_secs ? fmtAge(p.age_secs) : "—"}</td>
      <td class="num">${p.tree_mem ? fmtBytes(p.tree_mem) : "—"}</td>
      <td class="num ${p.cpu > 25 ? "vhot" : p.cpu > 5 ? "hot" : ""}">${p.cpu >= 0.1 ? `${p.cpu.toFixed(1)}%` : "—"}</td>
      <td>${p.ports.length ? p.ports.slice(0, 4).map((x) => `<code>${x}</code>`).join(" ") + (p.ports.length > 4 ? " …" : "") : "–"}</td>
      <td>${badges}${keep}</td>
      <td class="num">${p.protected ? "" : `<button class="small ghost" data-end="${esc(k)}" aria-label="${esc(`${t("proc.end")} ${p.label} · PID ${p.pid}`)}">${esc(t("proc.end"))}</button>`}</td>
    </tr>`;
  }).join("");
}

wireSort("#tab-cleanup", "dev", renderDev);
$("#clean-body").addEventListener("change", (e) => {
  const k = e.target.dataset.dev;
  if (!k) return;
  if (e.target.checked) { devSel.add(k); devUnticked.delete(k); } else { devSel.delete(k); devUnticked.add(k); }
});
$("#btn-clean-scan").onclick = (e) => loadDev(e.target);
$("#clean-body").addEventListener("click", async (e) => {
  if (e.target.closest("[data-retry]")) return loadDev(e.target.closest("button"));
  const one = e.target.closest("[data-end]");
  if (one) {
    // end just this row's tree, with the same confirm and report as "End selected"
    const p = dev?.items.find((x) => devKey(x) === one.dataset.end && !x.protected);
    return p ? endDev([p], one) : undefined;
  }
  const b = e.target.closest("[data-keep]");
  if (!b) return;
  const on = b.dataset.keep === "1";
  b.disabled = true;
  try {
    const s = await call("set_dev_keep", { cmd: b.dataset.cmd, keep: on });
    if (state) state.settings = s;
    // a kept row is never pre-selected: drop it from the selection right away
    if (on) devSel.forEach((k) => { if (dev?.items.find((p) => devKey(p) === k)?.cmd === b.dataset.cmd) devSel.delete(k); });
    await loadDev(null, true);
  } catch { b.disabled = false; }
});
$("#btn-clean-stale").onclick = async (e) => {
  await loadDev(e.target); // fresh scan, so rows that just went stale are included
  if (!dev) return;
  const stale = dev.items.filter((p) => p.stale && !p.protected);
  stale.forEach((p) => { devSel.add(devKey(p)); devUnticked.delete(devKey(p)); });
  renderDev();
  if (!stale.length) toast(t("clean.noStale"));
};
$("#btn-clean-end").onclick = (e) => {
  const picked = (dev?.items || []).filter((p) => devSel.has(devKey(p)) && !p.protected);
  if (!picked.length) return toast(t("bc.nothing"));
  return endDev(picked, e.target.closest("button"));
};
async function endDev(picked, btn) {
  const serving = picked.filter((p) => p.ports.length).length;
  const agents = picked.filter((p) => p.kind === "agent").length;
  // what ending them gives back, as big numbers above the list
  const sum = (f) => picked.reduce((n, p) => n + f(p), 0);
  const ports = new Set(picked.flatMap((p) => p.ports)).size, cpu = sum((p) => p.cpu);
  const cpuSecs = sum((p) => p.cpu_secs || 0), runSecs = sum((p) => p.age_secs || 0);
  const stat = (k, v, cls = "") => `<div class="free-card ${cls}"><div class="big">${v}</div><div class="label">${esc(t(k))}</div></div>`;
  const cards = `<div class="free-cards">${stat("clean.free.ram", esc(fmtBytes(sum((p) => p.tree_mem))), "ram")}${stat("clean.free.procs", ltr(String(sum((p) => p.tree_count))))}${
    cpu >= 0.5 ? stat("clean.free.cpu", `${cpu.toFixed(0)}%`, "cpu") : ""}${ports ? stat("clean.free.ports", ltr(String(ports))) : ""}${
    cpuSecs >= 1 ? stat("clean.free.cpuTime", fmtDur(cpuSecs), "cpu") : ""}${runSecs >= 60 ? stat("clean.free.runTime", fmtDur(runSecs)) : ""}</div>`;
  const html = `${cards}<p>${esc(t("clean.confirm.body"))}</p><ul>${picked.map((p) =>
    `<li><b>${esc(p.label)}</b> <span class="sub"><bdi dir="ltr">${esc(p.project_path || devWhere(p) || "—")}</bdi>${p.ports.length ? " · :" + p.ports.slice(0, 3).join(", :") : ""} · ${fmtBytes(p.tree_mem)} · PID ${p.pid} (+${p.tree_count - 1})</span></li>`).join("")}</ul>
    ${serving ? `<p class="warn-text">${esc(t("clean.serving.warn", { n: serving }))}</p>` : ""}
    ${picked.some((p) => p.kind === "vm") ? `<p class="warn-text">${esc(t("clean.vm.warn"))}</p>` : ""}
    ${picked.some((p) => p.eco === "wsl" && p.kind !== "vm") ? `<p class="warn-text">${esc(t("clean.wsl.warn"))}</p>` : ""}
    ${agents ? `<p class="sub">💡 <code>claude --resume</code> / <code>codex resume</code></p>` : ""}`;
  if (!(await modal(t("clean.confirm.title", { n: picked.length }), html, { okText: t("clean.end"), danger: true }))) return;
  busy(btn, true);
  const estimate = picked.reduce((n, p) => n + p.tree_mem, 0);
  startRecovery(live.at(-1)?.mem_used ?? 0, Date.now(), estimate, false);
  try {
    const r = await call("kill_dev_processes", { targets: picked.map((p) => ({ pid: p.pid, start_time: p.start_time, label: p.label, project: p.project, project_path: p.project_path, ports: p.ports })) });
    noteFreed("dev", r.result.freed_mem);
    let msg = t("clean.done", { n: r.result.ended.length, mem: fmtBytes(r.result.freed_mem) });
    if (r.result.freed_cpu >= 0.5) msg += " · " + t("ready.cpu", { c: r.result.freed_cpu.toFixed(0) });
    if (r.result.cpu_secs >= 1) msg += "\n" + t("clean.done.cpu", { d: fmtDur(r.result.cpu_secs) });
    if (r.result.failed.length) msg += `\n\n${t("clean.failed")}\n• ` + r.result.failed.join("\n• ");
    toast(msg + `\n${t("clean.seeReport")}`, r.result.failed.length ? "warn" : "ok", 10000);
    reportPick = { before: r.before.id, after: r.after.id };
    if (r.result.mem_before && recovery) { recovery.before = r.result.mem_before; recovery.estimate = r.result.freed_mem; renderLive(); }
    picked.forEach((p) => devSel.delete(devKey(p)));
  } finally { if (btn.isConnected) busy(btn, false); }
  loadDev(null, true);
}

// ---------- browser cleanup ----------
let bc = null;            // BrowserReport
let bcSel = new Set();    // "browser|profile|key"
let bcSeen = new Set();
const bcKey = (b, p, i) => `${b.id}|${p.dir}|${i.key}`;
const bcRows = () => (bc?.browsers || []).flatMap((b) => b.profiles.flatMap((p) => p.items.map((i) => ({ b, p, i, k: bcKey(b, p, i) }))));
const bcPreselect = (r) => r.i.tier === "safe" && r.i.bytes >= 1048576 && !r.b.running;

async function loadBrowsers(btn, fresh = !!btn) {
  busy(btn, true, t("bc.scanning"));
  try { await scanOnce("browser", scanBrowsers, fresh); } finally { busy(btn, false); }
}
async function scanBrowsers() {
  if (!bc) $("#bc-body").innerHTML = `<tr><td colspan="6" class="loading"><span class="spinner"></span> ${esc(t("bc.scanning"))}</td></tr>`;
  try { [bc] = await Promise.all([call("get_browser_caches", undefined, { quiet: !bc }), loadVolumes()]); delete scanFailed.browser; }
  catch (e) { if (!bc) failRow("#bc-body", 6, "browser", e); throw e; }
  const rows = bcRows(), keys = new Set(rows.map((r) => r.k));
  bcSel = new Set([...bcSel].filter((k) => keys.has(k)));
  rows.forEach((r) => {
    if (r.b.running) bcSel.delete(r.k);
    else if (!bcSeen.has(r.k) && bcPreselect(r)) bcSel.add(r.k);
    bcSeen.add(r.k);
  });
  renderBrowsers();
}

function renderBrowsers() {
  if (!bc) return;
  if (!bc.browsers.length) {
    $("#bc-summary").textContent = "";
    $("#bc-body").innerHTML = `<tr><td colspan="6" class="loading">${esc(t("bc.none"))}</td></tr>`;
    return;
  }
  const picked = bcRows().filter((r) => bcSel.has(r.k)).reduce((a, r) => a + r.i.bytes, 0);
  $("#bc-summary").textContent = t("bc.summary", { safe: fmtBytes(bc.safe_total), site: fmtBytes(bc.site_total), sel: fmtBytes(picked) });
  renderBcSpace();
  const total = (p) => p.items.reduce((a, i) => a + i.bytes, 0);
  const bTotal = (b) => b.profiles.reduce((a, p) => a + total(p), 0);
  const sk = sorts.bc.key;
  // browsers stay grouped (biggest first); profiles follow the Profile/Size column, items follow the clicked column
  const profOrder = sorter("bc", (p) => sk === "profile" ? (p.dir === "" ? t("bc.shared") : p.name) : total(p));
  const itemOrder = sorter("bc", (i) => sk === "what" ? t("bc.item." + i.key) : sk === "type" ? i.tier : i.bytes);
  $("#bc-body").innerHTML = [...bc.browsers].sort((x, y) => bTotal(y) - bTotal(x)).map((b) => {
    const head = `<tr class="group"><td colspan="6"><b>${esc(b.name)}</b> <span class="sub">${fmtBytes(bTotal(b))}</span>
      ${b.running ? ` <span class="badge idle">${esc(t("bc.running"))}</span>${b.id === "edge" ? `<div class="sub">${esc(t("bc.edgeHint"))}</div>` : ""}` : ""}</td></tr>`;
    const profs = [...b.profiles].sort((x, y) => (x.dir === "") - (y.dir === "") || profOrder(x, y));
    return head + profs.map((p) => [...p.items].sort(sk === "profile" ? (x, y) => y.bytes - x.bytes : itemOrder).map((i, n) => {
      const k = bcKey(b, p, i);
      const pname = p.dir === "" ? t("bc.shared") : p.name;
      const showDir = p.dir && p.dir !== "." && p.dir !== p.name;
      return `<tr class="${b.running ? "locked" : ""}">
        <td>${b.running ? "🔒" : `<input type="checkbox" data-bc="${esc(k)}" ${bcSel.has(k) ? "checked" : ""} />`}</td>
        <td>${n === 0 ? `<b>${esc(pname)}</b>${showDir ? `<div class="sub"><bdi dir="ltr">${esc(p.dir)}</bdi></div>` : ""}` : ""}</td>
        <td>${esc(t("bc.item." + i.key))}</td>
        <td><span class="risk ${i.tier === "safe" ? "safe" : "caution"}">${esc(t("bc.tier." + i.tier))}</span></td>
        <td class="num">${fmtBytes(i.bytes)}</td>
        <td>${b.running || !i.bytes ? "" : `<button class="small ghost" data-icon="trash-2" data-bc-one="${esc(k)}">${esc(t("ready.clean"))}</button>`}</td>
      </tr>`;
    }).join("")).join("");
  }).join("");
}

wireSort("#tab-browser", "bc", renderBrowsers);
$("#bc-body").addEventListener("change", (e) => {
  const k = e.target.dataset.bc;
  if (k) { e.target.checked ? bcSel.add(k) : bcSel.delete(k); renderBrowsers(); }
});
$("#bc-body").addEventListener("click", (e) => {
  const one = e.target.closest("[data-bc-one]");
  const row = one && bcRows().find((r) => r.k === one.dataset.bcOne && !r.b.running);
  if (row) $("#btn-bc-clean").onclick({ target: one }, [row]);
});
$("#btn-bc-scan").onclick = (e) => loadBrowsers(e.target);
$("#btn-bc-safe").onclick = () => { bcRows().filter((r) => r.i.tier === "safe" && !r.b.running && r.i.bytes > 0).forEach((r) => bcSel.add(r.k)); renderBrowsers(); };
// only: clean just these rows (a row's own Clean button) instead of what's ticked
$("#btn-bc-clean").onclick = async (e, only) => {
  const picked = only || bcRows().filter((r) => bcSel.has(r.k) && !r.b.running);
  if (!picked.length) return toast(t("bc.nothing"));
  const size = picked.reduce((a, r) => a + r.i.bytes, 0);
  const site = picked.filter((r) => r.i.tier === "site");
  const html = `${spaceLines(sumBy(picked, sysDrive, (r) => r.i.bytes))}<p>${esc(t("bc.confirm.body"))}</p><ul>${picked.map((r) =>
    `<li><b>${esc(r.b.name)}</b> · ${esc(r.p.dir === "" ? t("bc.shared") : r.p.name)} · ${esc(t("bc.item." + r.i.key))} <span class="sub">${fmtBytes(r.i.bytes)}</span></li>`).join("")}</ul>
    ${site.length ? `<p class="warn-text">${esc(t("bc.site.warn"))}</p>` : ""}`;
  if (!(await modal(t("bc.confirm.title", { size: fmtBytes(size) }), html, { okText: t("bc.clean"), danger: site.length > 0 }))) return;
  busy(e.target.closest("button"), true);
  try {
    const r = await call("clean_browser_caches", { targets: picked.map((x) => ({ browser: x.b.id, profile: x.p.dir, key: x.i.key })) });
    noteFreed("bc", r.freed);
    await loadVolumes();
    let msg = t("bc.done", { mem: fmtBytes(r.freed) }) + spaceNow();
    if (r.locked_files) msg += "\n" + t("bc.locked", { n: r.locked_files });
    if (r.skipped_running.length) msg += "\n" + t("bc.skipped", { b: r.skipped_running.join(", ") });
    toast(msg, r.skipped_running.length ? "warn" : "ok", 9000);
  } finally { busy(e.target.closest("button"), false); }
  loadBrowsers(null, true);
};

// ---------- dev disk ----------
// Paths are only display data here: the backend re-verifies every (project, folder) or cache id.
function lines(v) { return v.split(/\r?\n/).map((x) => x.trim()).filter(Boolean); }
let disk = null;            // DiskReport
let diskSel = new Set();    // "c|id" or "p|path|name"
const diskSeen = new Set();
const cKey = (c) => `c|${c.id}`;
const aKey = (p, a) => `p|${p.path}|${a.name}`;
const wKey = (w) => `w|${w.path}`;
// git won't remove a worktree with changes, so it can't be picked; one whose repo is gone can
// only go to the Recycle Bin (picked by hand, after a warning)
const wLocked = (w) => w.busy || (w.dirty && !w.orphan);
// ---------- drive space meters (Dev Disk, Browser cleanup) ----------
// The page's main call to action: how full each drive is now, how full it will be after
// cleaning what's ticked, and one big button that does it.
let volumes = null;
async function loadVolumes() {
  try { volumes = await invoke("get_volumes"); } catch (e) { console.error("get_volumes", e); }
}
const driveOf = (p) => /^[a-z]:/i.test(p || "") ? p.slice(0, 2).toUpperCase() : null;
// picked / all: Map drive -> bytes. Only a drive where cleaning what's ticked visibly changes
// its free space gets a meter: "58 GB free → 58 GB free" promises nothing, so it isn't shown.
function renderSpace(el, picked, all, cta) {
  const vols = volumes || [];
  const gainOn = (v) => Math.min(picked.get(v.drive) || 0, v.total - v.free);
  const show = vols.filter((v) => gainOn(v) > 0 && fmtBytes(v.free) !== fmtBytes(v.free + gainOn(v)));
  el.classList.toggle("hidden", !show.length);
  if (!show.length) return;
  const total = [...picked.values()].reduce((a, b) => a + b, 0);
  const pct = (n, v) => Math.round((n / v.total) * 100);
  const rows = show.map((v) => {
    const used = v.total - v.free;
    const gain = Math.min(picked.get(v.drive) || 0, used);
    const max = Math.min(all.get(v.drive) || 0, used);
    const after = used - gain;
    const lvl = (u) => pct(u, v) >= 90 ? "bad" : pct(u, v) >= 75 ? "warn" : "";
    return `<div class="space-row">
      <div class="space-drive">💽 <b dir="ltr">${esc(v.drive)}</b></div>
      <div class="space-nums">
        <div class="space-now"><div class="label">${esc(t("space.now"))}</div>
          <div class="big ${lvl(used)}">${esc(t("space.free", { size: fmtBytes(v.free) }))}</div>
          <div class="sub">${esc(t("space.used", { pct: pct(used, v), total: fmtBytes(v.total) }))}</div></div>
        <div class="space-arrow" aria-hidden="true">→</div>
        <div class="space-after"><div class="label">${esc(t("space.after"))}</div>
          <div class="big ${gain ? "good" : lvl(after)}">${esc(t("space.free", { size: fmtBytes(v.free + gain) }))}${gain ? ` <bdi class="gain" dir="ltr">+${esc(fmtBytes(gain))}</bdi>` : ""}</div>
          <div class="sub">${esc(t("space.used", { pct: pct(after, v), total: fmtBytes(v.total) }))}</div></div>
      </div>
      <div class="space-bar" role="img" aria-label="${esc(t("space.aria", { d: v.drive, now: pct(used, v), after: pct(after, v) }))}">
        <i class="used ${lvl(after)}" style="width:${(after / v.total) * 100}%"></i><i class="gain" style="width:${(gain / v.total) * 100}%"></i><i class="more" style="width:${(Math.max(0, max - gain) / v.total) * 100}%"></i>
      </div>
      ${max > gain ? `<div class="sub">${esc(t("space.max", { size: fmtBytes(max), d: v.drive }))}</div>` : ""}
    </div>`;
  }).join("");
  const busyNow = cta.disabled;
  el.innerHTML = `<div class="space-rows">${rows}</div>
    <button class="primary space-cta" data-icon="trash-2" ${!total || busyNow ? "disabled" : ""}>${esc(total ? t("space.cta", { size: fmtBytes(total) }) : t("space.ctaNone"))}</button>`;
  el.querySelector(".space-cta").onclick = () => cta.click();
}
// anything without a drive letter of its own (a cache found by name only) counts toward the system drive
const sysDrive = () => (volumes || []).find((v) => v.system)?.drive;
const sumBy = (rows, drive, bytes) => rows.reduce((m, r) => { const d = drive(r) || sysDrive(); if (d) m.set(d, (m.get(d) || 0) + bytes(r)); return m; }, new Map());
const diskDrive = (r) => driveOf(r.c ? r.c.paths?.[0] : r.p ? r.p.path : r.w?.path);
// "C: 58 GB free → 81 GB free" lines for a confirm dialog (picked: Map drive -> bytes)
function spaceLines(picked) {
  return [...picked].map(([d, b]) => {
    const v = (volumes || []).find((x) => x.drive === d);
    return v ? `<p class="space-line">💽 <b dir="ltr">${esc(d)}</b> ${esc(t("space.line", { now: fmtBytes(v.free), after: fmtBytes(v.free + Math.min(b, v.total - v.free)) }))}</p>` : "";
  }).join("");
}
// after a cleanup: "C: now has 81 GB free" (volumes must be fresh)
function spaceNow() {
  const v = (volumes || []).find((x) => x.system);
  return v ? "\n" + t("space.nowFree", { d: v.drive, size: fmtBytes(v.free) }) : "";
}
function renderDiskSpace() {
  // an orphaned worktree goes to the Recycle Bin, which frees nothing until the bin is emptied
  const rows = diskRows().filter((r) => !r.w?.orphan);
  renderSpace($("#disk-space"), sumBy(rows.filter((r) => diskSel.has(r.k)), diskDrive, (r) => r.bytes), sumBy(rows, diskDrive, (r) => r.bytes), $("#btn-disk-clean"));
}
function renderBcSpace() {
  // browser profiles live under %LOCALAPPDATA%, on the system drive
  const rows = bcRows().filter((r) => !r.b.running);
  renderSpace($("#bc-space"), sumBy(rows.filter((r) => bcSel.has(r.k)), sysDrive, (r) => r.i.bytes), sumBy(rows, sysDrive, (r) => r.i.bytes), $("#btn-bc-clean"));
}

const diskRows = () => [
  ...(disk?.caches || []).filter((c) => !c.disabled && !c.busy && !c.show_only && c.bytes > 0).map((c) => ({ k: cKey(c), bytes: c.bytes, c })),
  ...(disk?.projects || []).filter((p) => !p.busy).flatMap((p) => p.items.map((a) => ({ k: aKey(p, a), bytes: a.bytes, p, a }))),
  ...(disk?.worktrees || []).filter((w) => !wLocked(w)).map((w) => ({ k: wKey(w), bytes: w.bytes, w })),
];
const shortPath = (p) => disk?.home && p.toLowerCase().startsWith(disk.home.toLowerCase() + "\\") ? "~" + p.slice(disk.home.length) : p;
const baseName = (p) => (p || "").replace(/[\\/]+$/, "").split(/[\\/]/).pop();
// a path (escaped HTML) with the project's own folder name shown as a pill inside it; when the
// path doesn't contain it (a worktree kept elsewhere), the pill goes in front
function pathPill(path, name) {
  const s = shortPath(path), pill = (n) => `<span class="path-pill">${esc(n)}</span>`;
  if (!name) return esc(s);
  const parts = s.split("\\"), i = parts.map((x) => x.toLowerCase()).lastIndexOf(name.toLowerCase());
  if (i < 0) return `${pill(name)} ${esc(s)}`;
  return parts.map((x, j) => j === i ? pill(x) : esc(x)).join("\\");
}
// Risky removals (a worktree is someone's checkout): a prompt the user pastes into their own AI
// agent, which checks the folder and says whether anything would be lost before they delete it.
function wtPrompt(ws) {
  const one = ws.length === 1;
  const lines = [
    one ? "I'm about to delete this git worktree to free disk space." : `I'm about to delete these ${ws.length} git worktrees to free disk space.`,
    "Before I do, check whether anything would be lost. Don't delete, commit, push or change anything: only inspect and report.",
    "",
  ];
  for (const w of ws) {
    lines.push(`Worktree: ${w.path}`);
    lines.push(w.repo ? `Repository: ${w.repo}` : "Repository: missing (moved or deleted), so it would go to the Recycle Bin");
    lines.push(`Branch: ${w.branch || "unknown"}`, `Size: ${fmtBytes(w.bytes)}`);
    if (w.tool) lines.push(`Probably created by: ${w.tool}`);
    if (w.dirty) lines.push("Has uncommitted changes");
    if (w.ignored?.length) lines.push(`Git-ignored files that would also be deleted: ${w.ignored.join(", ")}`);
    lines.push("");
  }
  lines.push(
    "Removal runs `git worktree remove`: the folder is deleted, the branch and its commits stay in the repository.",
    "",
    "Check, for each worktree:",
    "1. Uncommitted or untracked files: git -C \"<worktree>\" status --porcelain",
    "2. Commits on its branch that exist nowhere else (not pushed, not merged): git -C \"<worktree>\" log --oneline <branch> --not --remotes",
    "3. Stashes made from it: git -C \"<worktree>\" stash list",
    "4. Ignored files that can't be rebuilt (.env, local config, data, keys) versus ones that can (node_modules, target, dist, caches)",
    "5. Whether a terminal, editor, dev server or AI agent is still working in it",
    "",
    "Then answer SAFE TO DELETE or NOT SAFE for each one, list exactly what would be lost, and how to keep it (commit, push, copy a file).",
  );
  if (LANG !== "en") lines.push(`Reply in ${LANGS[LANG]}.`);
  return lines.join("\n");
}
async function copyText(s) {
  try { await navigator.clipboard.writeText(s); }
  catch {
    const ta = Object.assign(document.createElement("textarea"), { value: s });
    document.body.appendChild(ta); ta.select(); document.execCommand("copy"); ta.remove();
  }
}
// "Ask your AI" on a worktree row and in the clean-up confirmation (data-wt-ask = paths, one per line)
document.addEventListener("click", async (e) => {
  const b = e.target.closest("[data-wt-ask]");
  if (!b) return;
  const paths = b.dataset.wtAsk.split("\n");
  const ws = (disk?.worktrees || []).filter((w) => paths.includes(w.path));
  if (!ws.length) return;
  await copyText(wtPrompt(ws));
  toast(t("disk.wt.asked"), "ok");
});
// auto-select rules (Dev Disk bar): stale = untouched for N days; caches only the "rebuilds itself" ones
const diskAuto = () => {
  const s = state?.settings || {};
  return { days: s.disk_stale_days ?? 30, build: s.disk_auto_build ?? true, deps: s.disk_auto_deps ?? true,
    caches: s.disk_auto_caches ?? true, min: (s.disk_auto_min_mb ?? 50) * 1048576 };
};
const isStale = (p) => !p.busy && p.idle_days >= diskAuto().days;
const diskSuggested = (r) => {
  const a = diskAuto();
  if (r.w) return false; // a worktree is someone's checkout: always picked by hand
  if (r.bytes < a.min && r.a?.eco !== "leftover") return false;
  if (r.c) return a.caches && r.c.tier === "safe";
  if (r.a.eco === "leftover") return true;
  // the backend already rules out OneDrive, pnpm and venvs; recheck staleness and kind here
  const venv = r.a.eco === "python" && r.a.tier === "deps";
  return isStale(r.p) && !r.p.onedrive && !r.a.pnpm && !venv && (r.a.tier === "deps" ? a.deps : a.build);
};
const DA_DAYS = [3, 7, 14, 30, 60, 90, 180, 365], DA_MIN = [0, 10, 50, 100, 500, 1024];
function renderDiskAuto() {
  const a = diskAuto();
  const days = DA_DAYS.includes(a.days) ? DA_DAYS : [...DA_DAYS, a.days].sort((x, y) => x - y);
  $("#da-days").innerHTML = days.map((d) => `<option value="${d}">${esc(t("disk.auto.dayN", { n: d }))}</option>`).join("");
  $("#da-days").value = a.days;
  const mb = a.min / 1048576, mins = DA_MIN.includes(mb) ? DA_MIN : [...DA_MIN, mb].sort((x, y) => x - y);
  $("#da-min").innerHTML = mins.map((m) => `<option value="${m}">${m ? fmtBytes(m * 1048576) : "–"}</option>`).join("");
  $("#da-min").value = mb;
  $("#da-build").checked = a.build; $("#da-deps").checked = a.deps; $("#da-caches").checked = a.caches;
}
$("#disk-auto").addEventListener("change", async () => {
  const s = await call("set_disk_auto", {
    days: Number($("#da-days").value), build: $("#da-build").checked, deps: $("#da-deps").checked,
    caches: $("#da-caches").checked, minMb: Number($("#da-min").value),
  });
  if (state) state.settings = s;
  // the rules changed: the selection becomes exactly what they pick
  diskSel = new Set(diskRows().filter(diskSuggested).map((r) => r.k));
  renderDisk();
});
const DISK_ECO = { node: "Node", rust: "Rust", java: "Java", dotnet: ".NET", python: "Python", browser: "", go: "Go", leftover: "" };

// a tab switch waits for the scan in flight; the Scan button or a finished clean-up starts a new one
function loadDisk(fresh = false) {
  return scanOnce("disk", scanDisk, fresh);
}
async function scanDisk() {
  const btn = $("#btn-disk-scan");
  busy(btn, true, t("bc.scanning"));
  if (!disk) $("#disk-body").innerHTML = `<tr><td colspan="6" class="loading"><span class="spinner"></span> ${esc(t("disk.scanning"))}</td></tr>`;
  try { [disk] = await Promise.all([call("get_dev_disk", undefined, { quiet: !disk }), loadVolumes()]); delete scanFailed.disk; }
  catch (e) { if (!disk) failRow("#disk-body", 6, "disk", e); throw e; }
  finally { busy(btn, false); }
  const rows = diskRows(), keys = new Set(rows.map((r) => r.k));
  diskSel = new Set([...diskSel].filter((k) => keys.has(k)));
  rows.forEach((r) => { if (!diskSeen.has(r.k) && diskSuggested(r)) diskSel.add(r.k); diskSeen.add(r.k); });
  renderDisk();
}

// All / Active / Excluded: excluded = caches set to "Never clean" plus folders taken out of the scan
let diskView = "all";
function renderDiskView() {
  const n = (disk?.caches.filter((c) => c.disabled).length || 0) + (state?.settings?.disk_excludes?.length || 0);
  if (n === 0 && diskView === "excluded") diskView = "all";
  $("#disk-view").innerHTML = [["all", "disk.view.all"], ["active", "disk.view.active"], ["excluded", "disk.view.excluded"]]
    .map(([v, k]) => `<button role="radio" aria-checked="${diskView === v}" data-disk-view="${v}">${esc(t(k))}${v === "excluded" ? ` <span class="count">${n}</span>` : ""}</button>`).join("");
  syncRadios($("#disk-view"));
}
// segmented switchers are radio groups: one Tab stop (the checked option), arrows move the choice
function syncRadios(group) {
  const rs = [...group.querySelectorAll('[role="radio"]')];
  const on = rs.find((r) => r.getAttribute("aria-checked") === "true") || rs[0];
  rs.forEach((r) => { r.tabIndex = r === on ? 0 : -1; });
}
document.addEventListener("keydown", (e) => {
  const r = e.target.closest?.('[role="radiogroup"] [role="radio"]');
  if (!r || !["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"].includes(e.key)) return;
  const group = r.closest('[role="radiogroup"]'), rs = [...group.querySelectorAll('[role="radio"]')], i = rs.indexOf(r);
  const rtl = document.documentElement.dir === "rtl";
  const step = { ArrowRight: rtl ? -1 : 1, ArrowLeft: rtl ? 1 : -1, ArrowDown: 1, ArrowUp: -1 }[e.key];
  const n = e.key === "Home" ? 0 : e.key === "End" ? rs.length - 1 : (i + step + rs.length) % rs.length;
  e.preventDefault();
  rs[n].click(); // the group may redraw its buttons: focus the option now at that position
  group.querySelectorAll('[role="radio"]')[n]?.focus();
});
$("#disk-view").addEventListener("click", (e) => {
  const b = e.target.closest("[data-disk-view]");
  if (b) { diskView = b.dataset.diskView; renderDisk(); }
});

function renderDisk() {
  renderDiskAuto();
  renderDiskJob();
  renderDiskView();
  if (!disk) return;
  if (diskView === "excluded") return renderDiskExcluded();
  const picked = diskRows().filter((r) => diskSel.has(r.k)).reduce((a, r) => a + r.bytes, 0);
  $("#disk-summary").textContent = t("disk.summary", {
    caches: fmtBytes(disk.cache_total), projects: fmtBytes(disk.project_total), n: disk.projects.length,
    stale: fmtBytes(disk.projects.filter(isStale).reduce((a, p) => a + p.items.reduce((b, i) => b + i.bytes, 0), 0)), sel: fmtBytes(picked),
  });
  renderDiskSpace();
  // one row's own Clean button: the same confirm and backend job as "Delete selected", for just that row
  const oneBtn = (k) => `<button class="small ghost" data-icon="trash-2" data-disk-one="${esc(k)}" ${jobActive(diskJob) ? "disabled" : ""}>${esc(t("ready.clean"))}</button>`;
  const chk = (k, locked) => locked ? "🔒" : `<input type="checkbox" data-disk="${esc(k)}" ${diskSel.has(k) ? "checked" : ""} />`;
  const tier = (x) => `<span class="risk ${x === "safe" || x === "build" ? "safe" : x === "history" || x === "bin" ? "caution" : x === "shown" ? "" : "moderate"}">${esc(t("disk.tier." + x))}</span>`;
  let html = "";
  // package caches, then GPU & machine learning, then AI tools & editors; each its own group
  const cacheGroup = (c) => c.eco === "gpu" || c.eco === "ml" ? "gpu" : c.eco === "ai" ? "ai" : c.eco === "win" ? "win" : "pkg";
  const cacheOrder = sorter("disk", (c, k) => k === "what" ? t("disk.cache." + c.id) : k === "type" ? c.tier : k === "idle" ? (c.disabled ? -1 : c.busy ? 1 : 0) : c.bytes);
  for (const [g, title] of [["win", "disk.caches.win"], ["pkg", "disk.caches"], ["gpu", "disk.caches.gpu"], ["ai", "disk.caches.ai"]]) {
    const list = disk.caches.filter((c) => cacheGroup(c) === g && !(diskView === "active" && c.disabled));
    if (!list.length) continue;
    const freeable = list.filter((c) => !c.disabled && !c.show_only).reduce((a, c) => a + c.bytes, 0);
    html += `<tr class="group"><td colspan="6"><b>${esc(t(title))}</b> <span class="sub">${fmtBytes(freeable)}</span></td></tr>`;
    html += [...list].sort(cacheOrder).map((c) => {
      const info = [];
      if (c.show_only) info.push(esc(t("disk.showOnly")));
      if (c.disabled) info.push(`<span class="badge">${esc(t("disk.disabled"))}</span>`);
      if (c.busy && !c.show_only) info.push(`<span class="badge idle">${esc(t("disk.cacheBusy"))}</span>`);
      if (c.keep_newest) info.push(esc(t("disk.keepNewest", { total: fmtBytes(c.total) })));
      else if (c.aged_days) info.push(esc(t("disk.agedOnly", { d: c.aged_days, total: fmtBytes(c.total) })));
      else if (c.total > c.bytes && !c.show_only) info.push(esc(t(c.id === "conda" ? "disk.condaShared" : "disk.pnpmShared", { total: fmtBytes(c.total) })));
      if (c.tool) info.push(esc(t("disk.tool", { c: c.tool })));
      const size = c.show_only ? c.total : c.bytes;
      return `<tr class="${c.disabled || (c.busy && !c.show_only) ? "locked" : ""}" title="${esc(c.paths.join("\n"))}">
        <td>${c.show_only ? `<span aria-label="${esc(t("disk.showOnly"))}">👁</span>` : c.disabled ? "🚫" : chk(cKey(c), c.busy || c.bytes === 0)}</td>
        <td>${esc(t("disk.cache." + c.id))}</td><td>${tier(c.show_only ? "shown" : c.tier)}</td>
        <td class="num">${fmtBytes(size)}</td><td class="sub">${info.join(" · ")}</td>
        <td><div class="row-acts">${c.show_only || c.disabled || c.busy || !c.bytes ? "" : oneBtn(cKey(c))}${c.show_only ? "" : `<button class="small ghost" data-disk-toggle="${esc(c.id)}" data-on="${c.disabled ? 1 : 0}">${esc(t(c.disabled ? "disk.enable" : "disk.disable"))}</button>`}</div></td>
      </tr>`;
    }).join("");
  }
  if (disk.vdisks?.length && diskView !== "active") {
    html += `<tr class="group"><td colspan="6"><b>${esc(t("disk.vdisks"))}</b> <span class="sub">${fmtBytes(disk.vdisks.reduce((a, v) => a + v.bytes, 0))}</span></td></tr>`;
    html += disk.vdisks.map((v) => `<tr title="${esc(v.path)}">
        <td><span aria-hidden="true">💽</span></td>
        <td>${esc(v.kind === "docker" ? t("disk.vd.docker") : t("disk.vd.wsl", { n: v.name }))}</td><td>${tier("compact")}</td>
        <td class="num">${fmtBytes(v.bytes)}</td><td class="sub">${esc(t(v.kind === "docker" ? "disk.vd.dockerInfo" : "disk.vd.wslInfo"))}</td>
        <td><button class="small ghost" data-icon="shrink" data-vd="${esc(v.path)}">${esc(t("disk.vd.compact"))}</button></td>
      </tr>`).join("");
  }
  if (disk.worktrees?.length) {
    html += `<tr class="group"><td colspan="6"><b>${esc(t("disk.worktrees"))}</b> <span class="sub">${fmtBytes(disk.worktree_total)}</span></td></tr>`;
    const wtOrder = sorter("disk", (w, k) => k === "what" ? shortPath(w.path) : k === "idle" ? w.idle_days : k === "type" ? w.tool : w.bytes);
    const TOOL = { claude: "Claude Code", cursor: "Cursor", codex: "Codex", windsurf: "Windsurf", git: "Git", temp: "%TEMP%" };
    html += [...disk.worktrees].sort(wtOrder).map((w) => {
      const badges = [
        w.busy ? `<span class="badge serving">${esc(t("disk.inUse"))}</span>` : "",
        w.orphan ? `<span class="badge orphan" title="${esc(t("disk.wt.orphanTip"))}">${esc(t("disk.wt.orphan"))}</span>` : "",
        w.dirty && !w.orphan ? `<span class="badge orphan">${esc(t("disk.wt.dirty"))}</span>` : "",
        w.idle_days >= diskAuto().days ? `<span class="badge idle">${esc(t("disk.untouched", { d: w.idle_days }))}</span>`
          : `<span class="badge active">${esc(w.idle_days === 0 ? t("disk.today") : t("disk.recent", { d: w.idle_days }))}</span>`,
      ].join("");
      const notes = [t("disk.wt.branch", { b: w.branch || "?" }), w.repo ? t("disk.wt.repo", { p: shortPath(w.repo) }) : ""];
      if (w.ignored.length) notes.push(t("disk.wt.ignored", { list: w.ignored.join(", ") }));
      return `<tr class="${wLocked(w) ? "locked" : ""}" title="${esc(w.path)}">
        <td>${chk(wKey(w), wLocked(w))}</td>
        <td><b><bdi dir="ltr">${pathPill(w.path, baseName(w.repo))}</bdi></b> <span class="sub">${esc(TOOL[w.tool] || w.tool)}</span></td>
        <td>${tier("worktree")}</td><td class="num">${fmtBytes(w.bytes)}</td>
        <td>${badges} <span class="sub">${esc(notes.filter(Boolean).join(" · "))}</span></td>
        <td><button class="small ghost" data-icon="copy" data-wt-ask="${esc(w.path)}" title="${esc(t("disk.wt.askTip"))}">${esc(t("disk.wt.ask"))}</button></td>
      </tr>`;
    }).join("");
  }
  html += `<tr class="group"><td colspan="6"><b>${esc(t("disk.projects"))}</b> <span class="sub">${fmtBytes(disk.project_total)}</span></td></tr>`;
  if (!disk.projects.length) html += `<tr><td colspan="6" class="loading">${esc(t("disk.none", { roots: disk.roots.map(shortPath).join(", ") }))}</td></tr>`;
  const pTotal = (p) => p.items.reduce((a, i) => a + i.bytes, 0);
  const dk = sorts.disk.key;
  // projects follow the clicked column; the folders inside a project follow it too when it is Name/Type, else biggest first
  const projOrder = sorter("disk", (p, k) => k === "what" ? shortPath(p.path) : k === "idle" ? p.idle_days : k === "type" ? p.items.map((i) => i.tier).sort().join() : pTotal(p));
  const itemOrder = dk === "what" || dk === "type" ? sorter("disk", (a, k) => k === "what" ? a.name : a.tier) : (x, y) => y.bytes - x.bytes;
  html += [...disk.projects].sort(projOrder).map((p) => {
    const total = pTotal(p);
    const badges = [
      p.busy ? `<span class="badge serving">${esc(t("disk.inUse"))}</span>` : "",
      isStale(p) ? `<span class="badge idle">${esc(t("disk.untouched", { d: p.idle_days }))}</span>`
        : `<span class="badge active">${esc(p.idle_days === 0 ? t("disk.today") : t("disk.recent", { d: p.idle_days }))}</span>`,
      p.onedrive ? `<span class="badge orphan">${esc(t("disk.onedrive"))}</span>` : "",
    ].join("");
    const head = `<tr class="group"><td></td><td colspan="2"><b title="${esc(p.path)}"><bdi dir="ltr">${pathPill(p.path, baseName(p.path))}</bdi></b></td>
      <td class="num sub">${fmtBytes(total)}</td><td>${badges}</td>
      <td><button class="small ghost" data-disk-exclude="${esc(p.path)}">${esc(t("disk.exclude"))}</button></td></tr>`;
    return head + [...p.items].sort(itemOrder).map((a) => {
      const note = a.eco === "leftover" ? t("disk.leftover") : a.pnpm ? t("disk.pnpm") : a.eco === "python" && a.tier === "deps" ? t("disk.venv") : "";
      return `<tr class="${p.busy ? "locked" : ""}">
        <td>${chk(aKey(p, a), p.busy)}</td><td><code>${esc(a.name)}</code> <span class="sub">${esc(DISK_ECO[a.eco] ?? a.eco)}</span></td>
        <td>${tier(a.tier)}</td><td class="num">${fmtBytes(a.bytes)}</td><td class="sub">${esc(note)}</td><td>${p.busy || !a.bytes ? "" : oneBtn(aKey(p, a))}</td>
      </tr>`;
    }).join("");
  }).join("");
  $("#disk-body").innerHTML = html;
}

// the Excluded view: what RigSweep leaves alone, each with a way back
function renderDiskExcluded() {
  const caches = disk.caches.filter((c) => c.disabled);
  const folders = state?.settings?.disk_excludes || [];
  let html = "";
  if (caches.length) {
    html += `<tr class="group"><td colspan="6"><b>${esc(t("disk.ex.caches", { x: t("disk.disable") }))}</b></td></tr>`;
    html += caches.map((c) => `<tr title="${esc(c.paths.join("\n"))}"><td>🚫</td><td>${esc(t("disk.cache." + c.id))}</td>
      <td><span class="badge">${esc(t("disk.disabled"))}</span></td><td class="num">${fmtBytes(c.bytes)}</td><td></td>
      <td><button class="small ghost" data-disk-toggle="${esc(c.id)}" data-on="1">${esc(t("disk.enable"))}</button></td></tr>`).join("");
  }
  if (folders.length) {
    html += `<tr class="group"><td colspan="6"><b>${esc(t("disk.ex.folders"))}</b> <span class="sub">${esc(t("disk.ex.foldersHint"))}</span></td></tr>`;
    html += folders.map((f) => `<tr><td>🚫</td><td colspan="2"><b title="${esc(f)}"><bdi dir="ltr">${esc(shortPath(f))}</bdi></b></td>
      <td class="num sub">—</td><td><span class="badge">${esc(t("disk.ex.notScanned"))}</span></td>
      <td><button class="small ghost" data-disk-include="${esc(f)}">${esc(t("disk.ex.include"))}</button></td></tr>`).join("");
  }
  if (!html) html = `<tr><td colspan="6" class="loading">${esc(t("disk.ex.none"))}</td></tr>`;
  $("#disk-body").innerHTML = html;
}

wireSort("#tab-disk", "disk", renderDisk);
$("#disk-body").addEventListener("change", (e) => {
  const k = e.target.dataset.disk;
  if (k) { e.target.checked ? diskSel.add(k) : diskSel.delete(k); renderDisk(); }
});
$("#disk-body").addEventListener("click", async (e) => {
  const tg = e.target.closest("[data-disk-toggle]"), ex = e.target.closest("[data-disk-exclude]"), inc = e.target.closest("[data-disk-include]");
  const vd = e.target.closest("[data-vd]"), one = e.target.closest("[data-disk-one]");
  if (vd) return compactVdisk(vd);
  if (one) {
    const row = diskRows().find((r) => r.k === one.dataset.diskOne);
    return row ? $("#btn-disk-clean").onclick({ target: one }, [row]) : undefined;
  }
  if (inc) {
    state.settings = await call("remove_disk_exclude", { path: inc.dataset.diskInclude });
    toast(t("disk.ex.included", { p: shortPath(inc.dataset.diskInclude) }), "ok");
    renderDisk();
    loadDisk(true);
  } else if (tg) {
    const id = tg.dataset.diskToggle, enabled = tg.dataset.on === "1";
    state.settings = await call("set_disk_cache_enabled", { id, enabled });
    const c = disk.caches.find((x) => x.id === id);
    if (c) c.disabled = !enabled;
    if (!enabled) diskSel.delete(`c|${id}`);
    disk.cache_total = disk.caches.filter((x) => !x.disabled && !x.show_only).reduce((a, x) => a + x.bytes, 0);
    renderDisk();
  } else if (ex) {
    const path = ex.dataset.diskExclude;
    if (!(await modal(t("disk.excludeConfirm", { p: shortPath(path) }), `<p>${esc(t("disk.excludeBody"))}</p>`, { okText: t("disk.exclude") }))) return;
    state.settings = await call("add_disk_exclude", { path });
    const low = path.toLowerCase();
    disk.projects = disk.projects.filter((p) => { const l = p.path.toLowerCase(); return l !== low && !l.startsWith(low + "\\"); });
    [...diskSel].filter((k) => k.startsWith("p|") && !diskRows().some((r) => r.k === k)).forEach((k) => diskSel.delete(k));
    toast(t("disk.excluded", { p: shortPath(path) }), "ok");
    renderDisk();
  }
});
// Docker Desktop / WSL disks: shrink the .vhdx (nothing inside is deleted unless pruning is ticked)
async function compactVdisk(btn) {
  const v = (disk?.vdisks || []).find((x) => x.path === btn.dataset.vd);
  if (!v) return;
  if (jobActive(diskJob)) return toast(t("disk.job.busy"), "warn");
  const docker = v.kind === "docker";
  const html = `<p>${esc(t("disk.vd.body"))}</p>
    <p class="warn-text">${esc(t(docker ? "disk.vd.warnDocker" : "disk.vd.warnWsl"))}</p>
    ${docker ? `<label class="check-row"><input type="checkbox" id="vd-prune" checked /> <span>${esc(t("disk.vd.prune"))}</span></label>` : ""}
    <p class="sub">${esc(t("disk.vd.slow"))}</p>`;
  const title = t("disk.vd.confirm", { n: docker ? t("disk.vd.docker") : v.name, size: fmtBytes(v.bytes) });
  if (!(await modal(title, html, { okText: t("disk.vd.compact") }))) return;
  const prune = docker && !!$("#vd-prune")?.checked;
  busy(btn, true, t("disk.vd.running"));
  try {
    const r = await call("compact_vdisk", { path: v.path, prune });
    const got = Math.max(0, r.before - r.after);
    noteFreed("disk", got);
    await loadVolumes();
    toast(t("disk.vd.done", { before: fmtBytes(r.before), after: fmtBytes(r.after), got: fmtBytes(got) }) + spaceNow(), "ok", 10000);
  } finally { busy(btn, false); }
  loadDisk(true);
}

$("#btn-disk-scan").onclick = () => loadDisk(true);
$("#btn-disk-suggest").onclick = () => { diskRows().filter(diskSuggested).forEach((r) => diskSel.add(r.k)); renderDisk(); };
// only: clean just these rows (a row's own Clean button) instead of what's ticked
$("#btn-disk-clean").onclick = async (e, only) => {
  if (jobActive(diskJob)) return toast(t("disk.job.busy"), "warn");
  const picked = only || diskRows().filter((r) => diskSel.has(r.k));
  if (!picked.length) return toast(t("disk.nothing"));
  const size = picked.reduce((a, r) => a + r.bytes, 0);
  const deps = picked.some((r) => r.a?.tier === "deps");
  const redl = picked.some((r) => r.c?.tier === "redownload");
  const hist = picked.some((r) => r.c?.tier === "history" || r.c?.tier === "backup");
  const wts = picked.filter((r) => r.w);
  const od = picked.some((r) => r.p?.onedrive);
  const li = picked.map((r) => r.c
    ? `<li>${esc(t("disk.cache." + r.c.id))} <span class="sub">${fmtBytes(r.bytes)}</span></li>`
    : r.w ? `<li><bdi dir="ltr">${pathPill(r.w.path, baseName(r.w.repo))}</bdi> <span class="sub">${fmtBytes(r.bytes)}</span></li>`
    : `<li><bdi dir="ltr">${pathPill(r.p.path, baseName(r.p.path))}\\<b>${esc(r.a.name)}</b></bdi> <span class="sub">${fmtBytes(r.bytes)}</span></li>`).join("");
  const ign = [...new Set(wts.flatMap((r) => r.w.ignored))];
  const html = `${spaceLines(sumBy(picked.filter((r) => !r.w?.orphan), diskDrive, (r) => r.bytes))}<p>${esc(t("disk.confirm.body"))}</p><ul class="scroll-list">${li}</ul>
    ${deps ? `<p class="warn-text">${esc(t("disk.deps.warn"))}</p>` : ""}
    ${redl ? `<p class="warn-text">${esc(t("disk.redl.warn"))}</p>` : ""}
    ${hist ? `<p class="warn-text">${esc(t("disk.hist.warn"))}</p>` : ""}
    ${wts.some((r) => !r.w.orphan) ? `<p class="warn-text">${esc(t("disk.wt.warn"))}</p>` : ""}
    ${wts.some((r) => r.w.orphan) ? `<p class="warn-text">${esc(t("disk.wt.orphanWarn"))}</p>` : ""}
    ${ign.length ? `<p class="warn-text">${esc(t("disk.wt.ignored", { list: ign.join(", ") }))}</p>` : ""}
    ${wts.length ? `<p><button class="small ghost" data-icon="copy" data-wt-ask="${esc(wts.map((r) => r.w.path).join("\n"))}">${esc(t("disk.wt.askAll"))}</button> <span class="sub">${esc(t("disk.wt.askTip"))}</span></p>` : ""}
    ${od ? `<p class="warn-text">${esc(t("disk.od.warn"))}</p>` : ""}`;
  if (!(await modal(t("disk.confirm.title", { size: fmtBytes(size) }), html, { okText: t("disk.clean"), danger: true }))) return;
  const targets = picked.map((r) => r.c
    ? { kind: "cache", id: r.c.id, name: "", bytes: r.bytes }
    : r.w ? { kind: "worktree", id: r.w.path, name: r.w.orphan ? "recycle" : "", bytes: r.bytes }
    : { kind: "project", id: r.p.path, name: r.a.name, bytes: r.bytes });
  // the job runs in the backend; a reload or tab switch just picks it up again
  try { applyDiskJob(await call("start_disk_clean", { targets })); } catch { pollDiskJob(); return; }
  pollDiskJob();
};

// ---------- dev disk cleanup job (lives in the backend; this side only polls and draws it) ----------
let diskJob = null, diskJobTimer = null, diskRate = null;
const JOB_ACTIVE = ["running", "paused", "stopping"];
const jobActive = (j) => !!j && JOB_ACTIVE.includes(j.state);

async function pollDiskJob() {
  clearTimeout(diskJobTimer);
  let j = null;
  try { j = await invoke("get_disk_job"); } catch (e) { console.error("get_disk_job", e); }
  if (j) applyDiskJob(j);
  // keep polling while it runs; after a failed poll try again a bit later
  if (!j || jobActive(j)) diskJobTimer = setTimeout(pollDiskJob, !j ? 2000 : j.state === "paused" ? 800 : 300);
}

function applyDiskJob(j) {
  const prev = diskJob;
  diskJob = j;
  if (jobActive(j)) {
    const now = performance.now();
    if (!diskRate || diskRate.id !== j.id) diskRate = { id: j.id, t: now, freed: j.freed, bps: 0 };
    else if (now - diskRate.t >= 900) {
      const bps = Math.max(0, (j.freed - diskRate.freed) / ((now - diskRate.t) / 1000));
      diskRate = { id: j.id, t: now, freed: j.freed, bps: diskRate.bps ? diskRate.bps * 0.6 + bps * 0.4 : bps };
    }
  }
  // it ended while this page watched it: report, drop what's gone, rescan
  if (prev && prev.id === j.id && jobActive(prev) && !jobActive(j) && j.result) onDiskJobEnd(j);
  renderDiskJob();
}

function onDiskJobEnd(j) {
  const r = j.result;
  noteFreed("disk", r.freed);
  let msg = j.state === "stopped" ? t("disk.job.stoppedToast", { mem: fmtBytes(r.freed) }) : t("disk.done", { mem: fmtBytes(r.freed) });
  if (r.locked_files) msg += "\n" + t("disk.locked", { n: r.locked_files });
  if (r.skipped.length) msg += "\n" + t("disk.skipped") + "\n• " + r.skipped.map(shortPath).join("\n• ");
  // the toast waits for fresh drive sizes, so it can say how much is free now
  loadVolumes().then(() => { renderDisk(); toast(msg + spaceNow(), r.skipped.length ? "warn" : "ok", 10000); });
  if (disk) {
    // drop what's gone at once; the rescan below (slow on big trees) only refreshes the sizes
    const gone = new Set(r.done);
    gone.forEach((k) => diskSel.delete(k));
    disk.caches.forEach((c) => { if (gone.has(cKey(c))) { c.bytes = 0; c.total = 0; } });
    disk.caches = disk.caches.filter((c) => c.total > 0);
    disk.projects.forEach((p) => { p.items = p.items.filter((a) => !gone.has(aKey(p, a))); });
    disk.projects = disk.projects.filter((p) => p.items.length);
    disk.worktrees = (disk.worktrees || []).filter((w) => !gone.has(wKey(w)));
    disk.worktree_total = disk.worktrees.reduce((a, w) => a + w.bytes, 0);
    const sum = (p) => p.items.reduce((a, i) => a + i.bytes, 0);
    disk.cache_total = disk.caches.filter((c) => !c.disabled && !c.show_only).reduce((a, c) => a + c.bytes, 0);
    disk.project_total = disk.projects.reduce((a, p) => a + sum(p), 0);
    disk.stale_total = disk.projects.filter((p) => p.stale).reduce((a, p) => a + sum(p), 0);
    renderDisk();
  }
  if (currentTab === "disk" || disk) loadDisk(true);
}

// a long path is cut at the start: the file name is the part worth seeing
const tailPath = (p, n = 110) => !p ? "" : p.length > n ? "…" + p.slice(-n) : p;

function renderDiskJob() {
  const box = $("#disk-job"), j = diskJob, active = jobActive(j);
  const cleanBtn = $("#btn-disk-clean");
  cleanBtn.disabled = active;
  cleanBtn.title = active ? t("disk.job.busy") : "";
  const cta = $("#disk-space .space-cta");
  if (cta && active) cta.disabled = true;
  const resumeN = j?.resume?.length || 0;
  const show = !!j && (j.state !== "idle" || resumeN > 0);
  box.classList.toggle("hidden", !show);
  if (!show) return;
  const st = j.state === "idle" ? "stopped" : j.state;
  box.className = `jobbar ${st}`;
  const pct = j.total ? Math.min(100, (j.freed / j.total) * 100) : 0;
  const leftBytes = (j.resume || []).reduce((a, x) => a + (x.bytes || 0), 0);
  const title = {
    running: () => t("disk.job.running", { i: j.index + 1, n: j.count }),
    paused: () => t("disk.job.paused", { i: j.index + 1, n: j.count }),
    stopping: () => t("disk.job.stopping"),
    stopped: () => t(j.state === "idle" ? "disk.job.unfinished" : "disk.job.stopped", { n: resumeN, size: fmtBytes(leftBytes) }),
    done: () => t("disk.job.done", { mem: fmtBytes(j.freed), files: j.files.toLocaleString() }),
  }[st]();
  $("#dj-title").textContent = title;
  const bar = $("#dj-bar"), prog = box.querySelector(".progress");
  // running with no size hint (a tool's own cleanup): an indeterminate bar
  box.classList.toggle("indeterminate", j.state === "running" && !j.total);
  bar.style.width = (st === "done" ? 100 : pct) + "%";
  prog.setAttribute("aria-valuenow", String(Math.round(st === "done" ? 100 : pct)));
  prog.classList.toggle("hidden", j.state === "idle");
  const rate = active && j.state === "running" && diskRate?.id === j.id && diskRate.bps > 0 ? diskRate.bps : 0;
  $("#dj-stats").textContent = j.state === "idle" ? "" : t("disk.job.stats", {
    freed: fmtBytes(j.freed), total: fmtBytes(j.total), pct: Math.round(pct), files: j.files.toLocaleString(),
  }) + (rate ? ` · ${fmtBytes(rate)}/s` : "");
  const file = $("#dj-file");
  file.textContent = active ? tailPath(j.current) || shortPath(j.target) : "";
  file.title = active ? j.current : "";
  file.classList.toggle("hidden", !active);
  const pause = $("#dj-pause");
  pause.textContent = t(j.state === "paused" ? "disk.job.resume" : "disk.job.pause");
  pause.classList.toggle("hidden", !(j.state === "running" || j.state === "paused"));
  $("#dj-stop").classList.toggle("hidden", !(j.state === "running" || j.state === "paused"));
  $("#dj-continue").classList.toggle("hidden", active || !resumeN);
  $("#dj-dismiss").classList.toggle("hidden", active);
}

async function diskJobCall(btn, cmd, args) {
  btn.disabled = true;
  try { applyDiskJob(await call(cmd, args)); } catch {} finally { btn.disabled = false; }
  pollDiskJob();
}
$("#dj-pause").onclick = (e) => diskJobCall(e.currentTarget, "disk_job_control", { action: diskJob?.state === "paused" ? "resume" : "pause" });
$("#dj-stop").onclick = (e) => diskJobCall(e.currentTarget, "disk_job_control", { action: "stop" });
$("#dj-continue").onclick = (e) => diskJobCall(e.currentTarget, "resume_disk_clean");
$("#dj-dismiss").onclick = (e) => diskJobCall(e.currentTarget, "dismiss_disk_job");

// ---------- boost report ----------
let reportPick = null; // {before, after} snapshot ids
const METRICS = [
  // key, formatter, lower-is-better
  ["mem_used", fmtBytes, true],
  ["mem_free", fmtBytes, false],
  ["cpu_avg", (v) => `${v.toFixed(1)}%`, true],
  ["process_count", String, true],
  ["service_running", String, true],
  ["chrome_mem", fmtBytes, true],
  ["chrome_procs", String, true],
  ["dev_mem", fmtBytes, true],
  ["dev_procs", String, true],
];
const metricVal = (m, k) => k === "mem_free" ? m.mem_total - m.mem_used : m[k] ?? 0;
const snapName = (s) => t("label." + s.label) === "label." + s.label ? s.label : t("label." + s.label);

async function loadSnapshots() {
  snaps = await call("get_snapshots");
  const opts = snaps.map((s) => `<option value="${s.id}" title="${esc(fullDate(s.ts))}" data-ago="${esc(s.ts)}" data-ago-after="${esc(" · " + snapName(s))}">${esc(timeAgo(s.ts) + " · " + snapName(s))}</option>`).join("");
  $("#snap-before").innerHTML = opts;
  $("#snap-after").innerHTML = opts;
  // comparing needs two measurements; until then the pickers are only noise
  $(".pickers").classList.toggle("hidden", snaps.length < 2);
  if (snaps.length) {
    const base = snaps.find((s) => s.label === "baseline") || snaps[0];
    const pick = reportPick || { before: base.id, after: snaps[snaps.length - 1].id };
    $("#snap-before").value = pick.before;
    $("#snap-after").value = pick.after;
  }
  renderReport();
}

function currentPair() {
  const b = snaps.find((s) => s.id === Number($("#snap-before").value));
  const a = snaps.find((s) => s.id === Number($("#snap-after").value));
  return [b, a];
}

function renderReport() {
  const [b, a] = currentPair();
  const win = $("#report-win");
  if (!b || !a) {
    $("#report-body").innerHTML = `<div class="card loading">${esc(t("report.none"))}</div>`;
    win.classList.add("hidden");
    return;
  }
  $("#report-body").innerHTML = `<div class="card flush"><table class="tbl report"><thead><tr><th></th>
    <th class="num">${esc(t("report.before"))}</th><th class="num">${esc(t("report.after"))}</th><th class="num">${esc(t("report.change"))}</th><th class="barcol"></th></tr></thead><tbody>
    ${METRICS.map(([k, f, lower]) => {
      const vb = metricVal(b.metrics, k), va = metricVal(a.metrics, k), d = va - vb;
      const better = d === 0 ? "" : (d < 0) === lower ? "good" : "bad";
      const max = Math.max(vb, va, 1);
      const pct = vb ? ` (${d > 0 ? "+" : ""}${((d / vb) * 100).toFixed(0)}%)` : "";
      return `<tr><td>${esc(t("m." + k))}</td><td class="num">${f(vb)}</td><td class="num">${f(va)}</td>
        <td class="num ${better}">${d > 0 ? "+" : ""}${f(d)}${pct}</td>
        <td class="barcol"><div class="bar b" style="width:${(vb / max) * 100}%"></div><div class="bar a ${better}" style="width:${(va / max) * 100}%"></div></td></tr>`;
    }).join("")}</tbody></table></div>`;
  const freed = b.metrics.mem_used - a.metrics.mem_used;
  const fewer = b.metrics.process_count - a.metrics.process_count;
  win.classList.toggle("hidden", !(freed > 0 || fewer > 0));
  win.textContent = t("report.win", { mem: fmtBytes(Math.max(0, freed)), p: Math.max(0, fewer) });
}

function reportMarkdown() {
  const [b, a] = currentPair();
  if (!b || !a) return "";
  const rows = METRICS.map(([k, f]) => {
    const vb = metricVal(b.metrics, k), va = metricVal(a.metrics, k), d = va - vb;
    return `| ${t("m." + k)} | ${f(vb)} | ${f(va)} | ${d > 0 ? "+" : ""}${f(d)} |`;
  });
  return [`**RigSweep Boost Report** · ${fullDate(b.ts)} · ${snapName(b)} → ${fullDate(a.ts)} · ${snapName(a)}`, "",
    `| | ${t("report.before")} | ${t("report.after")} | ${t("report.change")} |`, "|---|---:|---:|---:|", ...rows].join("\n");
}

$("#snap-before").onchange = $("#snap-after").onchange = () => {
  reportPick = { before: Number($("#snap-before").value), after: Number($("#snap-after").value) };
  renderReport();
};
$("#btn-measure").onclick = async (e) => {
  busy(e.target.closest("button"), true);
  try {
    const s = await call("take_snapshot", { label: snaps.length ? "manual" : "baseline" });
    const base = reportPick?.before ?? (snaps.find((x) => x.label === "baseline") || snaps[0] || s).id;
    reportPick = { before: base, after: s.id };
  } finally { busy(e.target.closest("button"), false); }
  loadSnapshots();
};
$("#btn-copy-md").onclick = async () => {
  const md = reportMarkdown();
  if (!md) return toast(t("report.none"));
  try { await navigator.clipboard.writeText(md); }
  catch {
    const ta = Object.assign(document.createElement("textarea"), { value: md });
    document.body.appendChild(ta); ta.select(); document.execCommand("copy"); ta.remove();
  }
  toast(t("report.copied"), "ok");
};
$("#btn-clear-snaps").onclick = async () => {
  if (!(await modal(t("report.clear"), esc(t("report.clear.confirm")), { okText: t("report.clear"), danger: true }))) return;
  await call("clear_snapshots");
  reportPick = null;
  loadSnapshots();
};

// ---------- processes ----------
async function loadProcesses() {
  try { snapshot = await invoke("get_processes"); } catch (e) { return toast(`${t("err.failed")}\n${e}`, "err"); }
  renderProcesses();
}

function diskOf(p) { return p.disk_read + p.disk_write; }

function renderProcesses() {
  if (!snapshot) return;
  const q = $("#proc-filter").value.trim().toLowerCase();
  let rows = snapshot.processes.filter((p) => !q || p.name.toLowerCase().includes(q) || String(p.pid) === q ||
    p.services.some((s) => s.name.toLowerCase().includes(q) || s.display.toLowerCase().includes(q)));
  rows.sort(sorter("proc", (p, k) => k === "disk" ? diskOf(p) : p[k]));
  rows = rows.slice(0, 300);
  if (!rows.length) return keepFocus($("#proc-body"), () => { $("#proc-body").innerHTML = `<tr><td colspan="7" class="loading">${q ? esc(t("st.noMatch")) : ""}</td></tr>`; });
  keepFocus($("#proc-body"), () => { $("#proc-body").innerHTML = rows.map((p) => {
    const cpuCls = p.cpu > 25 ? "vhot" : p.cpu > 5 ? "hot" : "";
    const svcs = p.services.map((s) => {
      const btn = s.protected ? `<span class="sub" title="${esc(t("proc.protected"))}">🔒</span>`
        : s.tweak_id ? `<button data-goto="${esc(s.tweak_id)}">${esc(t("proc.tweak"))}</button>`
        : `<button data-disable="${esc(s.name)}" data-display="${esc(s.display)}">${esc(t("proc.disable"))}</button>`;
      return `<span class="svc ${s.tweak_id ? "known" : ""}" title="${esc(s.display)}">${esc(s.name)} ${btn}</span>`;
    }).join("");
    return `<tr>
      <td><div class="named">${ico(p.exe)}<div><b>${esc(p.name)}</b><div class="sub" title="${esc(p.exe)}"><bdi dir="ltr">${esc(p.exe.length > 60 ? "…" + p.exe.slice(-58) : p.exe)}</bdi></div></div></div></td>
      <td class="num">${p.pid}</td>
      <td class="num ${cpuCls}">${p.cpu.toFixed(1)}%</td>
      <td class="num">${fmtBytes(p.mem)}</td>
      <td class="num">${diskOf(p) ? fmtBytes(diskOf(p)) : "–"}</td>
      <td>${svcs}</td>
      <td><div class="row-acts"><button class="small ghost" data-why="${p.pid}">${esc(t("why.btn"))}</button>${p.protected ? `<span class="sub lock" title="${esc(t("proc.protected"))}">🔒</span>` : `<button class="small ghost" data-kill="${p.pid}" data-start="${p.start_time}" data-name="${esc(p.name)}">${esc(t("proc.end"))}</button>`}</div></td>
    </tr>`;
  }).join(""); });
}

$("#proc-filter").addEventListener("input", renderProcesses);
$("#btn-refresh-proc").onclick = loadProcesses;
wireSort("#tab-processes", "proc", renderProcesses);

$("#proc-body").addEventListener("click", async (e) => {
  const el = e.target;
  if (el.dataset.kill) {
    if (!(await modal(t("proc.endTitle"), `<p>${t("proc.endBody", { name: esc(el.dataset.name), pid: esc(el.dataset.kill) })}</p>`, { okText: t("proc.end"), danger: true }))) return;
    const name = await call("kill_process", { pid: Number(el.dataset.kill), startTime: Number(el.dataset.start) });
    toast(t("proc.ended", { name }), "ok");
    loadProcesses();
  } else if (el.dataset.goto) {
    jumpToTweak(el.dataset.goto);
  } else if (el.dataset.disable) {
    const html = `<p>${t("proc.disableBody", { d: esc(el.dataset.display), n: esc(el.dataset.disable) })}</p>
      <p class="warn-text">${esc(t("proc.disableWarn"))}</p>`;
    if (!(await modal(t("proc.disableTitle"), html, { okText: t("proc.disable").replace("…", ""), danger: true }))) return;
    const id = await call("add_custom_service", { name: el.dataset.disable, display: el.dataset.display });
    const r = await call("apply_tweaks", { ids: [id] });
    const res = r.results[0];
    toast(res?.ok ? t("proc.disabled", { n: el.dataset.disable }) : t("proc.problem", { m: res?.messages.join("; ") }), res?.ok ? "ok" : "err", 8000);
    selected.add(id);
    await loadState();
    loadProcesses();
  }
});

// ---------- app icons ----------
// ico(path) returns an <img> when the icon is known, otherwise a placeholder that is filled in once
// the backend has extracted it. Requests from one render are batched into a single call.
const icoCache = new Map();   // lower-cased path -> data URL, or "" when the file has no icon
const icoWanted = new Map();  // lower-cased path -> path, waiting for the next batch
let icoTimer = null;
function ico(path) {
  if (!path) return `<span class="ico"></span>`;
  const k = path.toLowerCase(), v = icoCache.get(k);
  if (v) return `<img class="ico" src="${v}" alt="" />`;
  if (v === undefined && !icoWanted.has(k)) {
    icoWanted.set(k, path);
    clearTimeout(icoTimer);
    icoTimer = setTimeout(fetchIcons, 20);
  }
  return `<span class="ico" data-ico="${esc(k)}"></span>`;
}
async function fetchIcons() {
  const batch = [...icoWanted.entries()];
  if (!batch.length) return;
  let res = {};
  try { res = await invoke("get_icons", { paths: batch.map(([, p]) => p) }); } catch {}
  // only PNG data URLs from the backend are kept, so nothing else can reach an <img src>
  const png = (u) => (typeof u === "string" && /^data:image\/png;base64,[A-Za-z0-9+/=]+$/.test(u) ? u : "");
  batch.forEach(([k, p]) => { icoCache.set(k, png(res[p])); icoWanted.delete(k); });
  document.querySelectorAll("span.ico[data-ico]").forEach((el) => {
    const url = icoCache.get(el.dataset.ico);
    if (url === undefined) return;
    if (url) el.outerHTML = `<img class="ico" src="${url}" alt="" />`;
    else el.removeAttribute("data-ico");
  });
}

// ---------- startup ----------
// Everything that starts on its own, including services and scheduled tasks Task Manager never lists.
// Turning one off creates a custom tweak (backup, Restore, watchdog re-apply all come for free).
let startup = null;
const ST_KINDS = ["service", "task", "run", "folder"];
// VPN clients and security agents: often managed by an employer, worth a second look
const ST_SECURITY = /vpn|forti|security|antivirus|anti-virus|endpoint|defend|crowdstrike|sentinelone|sophos|eset|kaspersky|norton|mcafee|bitdefender|zscaler|globalprotect|cisco/i;

async function loadStartup(btn, fresh = !!btn) {
  busy(btn, true);
  try { await scanOnce("startup", scanStartup, fresh); } finally { busy(btn, false); }
}
async function scanStartup() {
  if (!startup) $("#st-body").innerHTML = `<tr><td colspan="3" class="loading"><span class="spinner"></span> ${esc(t("st.scanning"))}</td></tr>`;
  try { startup = await call("get_startup", undefined, { quiet: !startup }); }
  catch (e) { if (!startup) return failRow("#st-body", 3, "startup", e); }
  renderStartup();
}

function stWhen(it) {
  if (it.kind === "service") return t("st.trig.boot");
  if (it.kind === "task") return it.triggers.length ? it.triggers.map((x) => tOr("st.trig." + x, x)).join(", ") : t("st.trig.other");
  return t("st.trig.logon") + " · " + t("st.scope." + it.scope);
}

function renderStartup() {
  if (!startup) return;
  const q = $("#st-filter").value.trim().toLowerCase();
  const rows = startup.filter((it) => !q || it.name.toLowerCase().includes(q) || it.command.toLowerCase().includes(q) || it.key.toLowerCase().includes(q));
  const hidden = startup.filter((it) => it.kind === "service" || it.kind === "task").length;
  $("#st-summary").textContent = startup.length ? t("st.summary", { n: startup.length, h: hidden }) : "";
  if (!rows.length) {
    $("#st-body").innerHTML = `<tr><td colspan="3" class="loading">${esc(t(startup.length ? "st.noMatch" : "st.none"))}</td></tr>`;
  } else {
    $("#st-body").innerHTML = ST_KINDS.map((k) => {
      const list = rows.filter((it) => it.kind === k).sort(sorter("st", (it, c) => c === "when" ? stWhen(it) : it.name));
      if (!list.length) return "";
      return `<tr class="group"><td colspan="3"><b>${esc(t("st.g." + k))}</b> <span class="sub">${list.length}</span></td></tr>` + list.map((it) => {
        const tw = it.tweak_id && state?.tweaks.find((x) => x.id === it.tweak_id);
        const btn = tw && !tw.custom
          ? `<button class="small ghost" data-goto="${esc(tw.id)}">${esc(t("proc.tweak"))}</button>`
          : `<button class="small ghost" data-st-kind="${esc(it.kind)}" data-st-key="${esc(it.key)}">${esc(t("st.off"))}</button>`;
        const cmd = it.command.length > 90 ? it.command.slice(0, 88) + "…" : it.command;
        return `<tr>
          <td><div class="named">${ico(it.exe)}<div><b>${esc(it.name)}</b>
            ${it.running ? `<span class="badge active">${esc(t("st.running"))}</span>` : ""}
            ${it.hardware ? `<span class="badge idle" title="${esc(t("st.driverWarn"))}">${esc(t("st.driver"))}</span>` : ""}
            <div class="sub" title="${esc(it.command + "\n" + it.location)}"><bdi dir="ltr">${esc(cmd)}</bdi></div></div></div></td>
          <td>${esc(stWhen(it))}</td>
          <td>${btn}</td>
        </tr>`;
      }).join("");
    }).join("");
  }
  renderStartupOff();
}

function renderStartupOff() {
  const off = (state?.tweaks || []).filter((x) => x.custom && x.in_profile);
  $("#st-off").innerHTML = off.length ? off.map((x) =>
    `<li><div class="what"><b>${esc(x.name)}</b></div><button class="small ghost" data-st-restore="${esc(x.id)}" data-name="${esc(x.name)}">${esc(t("st.restore"))}</button></li>`).join("")
    : `<li class="empty-state">${esc(t("st.offNone"))}</li>`;
}

wireSort("#tab-startup", "st", renderStartup);
$("#st-filter").addEventListener("input", renderStartup);
$("#btn-st-scan").onclick = (e) => loadStartup(e.target);

$("#st-body").addEventListener("click", async (e) => {
  const el = e.target.closest("button");
  if (!el) return;
  if (el.dataset.goto) {
    jumpToTweak(el.dataset.goto);
    return;
  }
  // two attributes: a separator character inside one attribute can be rewritten by the HTML parser
  const kind = el.dataset.stKind, key = el.dataset.stKey;
  if (!kind || key == null) return;
  const it = startup.find((x) => x.kind === kind && x.key === key);
  if (!it) return;
  const body = kind === "service" ? "st.offBody.service" : kind === "task" ? "st.offBody.task" : "st.offBody.run";
  const html = `<p>${esc(t(body))}</p>
    ${it.hardware ? `<p class="warn-text">${esc(t("st.driverWarn"))}</p>` : ""}
    ${ST_SECURITY.test(it.name + " " + it.command) ? `<p class="warn-text">${esc(t("st.secWarn"))}</p>` : ""}
    <p class="sub">${esc(t("st.backup"))}</p>`;
  if (!(await modal(t("st.offTitle", { name: it.name }), html, { okText: t("st.off").replace("…", ""), danger: it.hardware }))) return;
  busy(el, true);
  try {
    const id = await call("add_startup_tweak", { kind, key, display: it.name });
    const r = await call("apply_tweaks", { ids: [id] });
    const res = r.results[0];
    toast(res?.ok ? t("st.done", { name: it.name }) : t("proc.problem", { m: res?.messages.join("; ") }), res?.ok ? "ok" : "err", 8000);
    if (r.restore_point) toast(r.restore_point, "info", 8000);
    selected.add(id);
  } catch { busy(el, false); return; }
  await loadState();
  loadStartup(null, true);
});

$("#st-off").addEventListener("click", async (e) => {
  const el = e.target.closest("button[data-st-restore]");
  if (!el) return;
  busy(el, true);
  try {
    await call("remove_custom", { id: el.dataset.stRestore });
    selected.delete(el.dataset.stRestore);
    toast(t("st.restored", { name: el.dataset.name }), "ok");
  } catch { busy(el, false); return; }
  await loadState();
  loadStartup(null, true);
});

// ---------- console pop-ups (flashes.rs) ----------
// Console windows that opened and closed on their own: what ran, what started it, what ran inside.
// The same command from the same source is one row with a count, so a task firing every hour shows up as a pattern.
let flashes = null;
const flTs = (ms) => { const d = new Date(ms), p = (n) => String(n).padStart(2, "0"); return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`; };
const flDur = (ms) => ms < 1000 ? t("fl.ms", { n: ms }) : t("fl.s", { n: (ms / 1000).toLocaleString(LANG, { maximumFractionDigits: ms < 10000 ? 1 : 0 }) });
// the program's full path from its command line, for its icon
function flExe(cmd) {
  const c = String(cmd || "").replace(/^\\\?\?\\/, "").trim();
  const m = c.startsWith('"') ? c.slice(1).split('"')[0] : (c.match(/^.*?\.exe/i)?.[0] || c.split(/\s/)[0]);
  return /[\\/]/.test(m) ? m : "";
}
async function openFlashes() {
  const gen = tabGen;
  const r = await call("get_flashes").catch(() => null);
  if (!r || gen !== tabGen) return;
  flashes = r;
  renderFlashes();
}
function flSource(f) {
  if (f.source === "task") return f.task ? t("fl.by.task", { x: ltr(f.task) }) : t("fl.by.taskUnknown");
  if (f.source === "service") return t("fl.by.service", { x: ltr(f.source_name) });
  if (f.source === "explorer") return t("fl.by.explorer");
  if (f.source === "app") return ltr(f.source_name);
  return t("fl.by.unknown");
}
const flTaskOff = (path) => (state?.tweaks || []).some((x) => x.id === "custom.task." + path.toLowerCase() && x.in_profile);
function flNodes(list) {
  return `<ul class="fl-nodes">${list.map((n) => `<li><b>${esc(n.name)}</b> <span class="sub">PID ${n.pid}</span>${n.cmd ? `<div class="sub mono-cut"><bdi dir="ltr">${esc(n.cmd)}</bdi></div>` : ""}</li>`).join("")}</ul>`;
}
function renderFlashes() {
  if (!flashes) return;
  const s = flashes.status;
  $("#fl-status").innerHTML = s.on ? `<i class="pulse"></i><span>${esc(t("fl.on"))}</span>`
    : `<span class="warn-text">${esc(s.error === "admin" ? t("fl.admin") : s.error ? t("fl.err", { m: s.error }) : t("fl.starting"))}</span>`;
  const groups = new Map();
  flashes.rows.forEach((f) => {
    const k = [f.name, f.cmd, f.task || "", f.source, f.source_name].join("\u0001");
    const g = groups.get(k);
    if (g) g.n++; else groups.set(k, { f, n: 1 });
  });
  $("#btn-fl-clear").disabled = !flashes.rows.length;
  if (!groups.size) {
    $("#fl-body").innerHTML = `<tr><td colspan="5" class="loading">${esc(t("fl.none"))}</td></tr>`;
    return;
  }
  $("#fl-body").innerHTML = [...groups.values()].map(({ f, n }) => {
    const cmd = f.cmd.length > 140 ? f.cmd.slice(0, 138) + "…" : f.cmd;
    const taskBtn = f.task && f.task_user
      ? (flTaskOff(f.task) ? `<span class="badge active">${esc(t("fl.taskOff"))}</span>` : `<button class="small ghost" data-fl-task="${esc(f.task)}">${esc(t("st.off"))}</button>`)
      : "";
    return `<tr>
      <td class="fl-when">${agoHtml(flTs(f.t))}${n > 1 ? `<div><span class="badge idle">${esc(t("fl.times", { n }))}</span></div>` : ""}</td>
      <td><div class="named">${ico(flExe(f.cmd))}<div><b>${esc(f.name)}</b>
        <div class="sub mono-cut" title="${esc(f.cmd)}"><bdi dir="ltr">${esc(cmd)}</bdi></div>
        <details class="chg"><summary>${esc(t("fl.details"))}</summary>
          ${f.chain.length ? `<h3>${esc(t("fl.chain"))}</h3>${flNodes(f.chain)}` : ""}
          ${f.children.length ? `<h3>${esc(t("fl.inside"))}</h3>${flNodes(f.children)}` : ""}
          ${f.exit != null ? `<p class="sub">${esc(t("fl.exit", { x: f.exit }))}</p>` : ""}
        </details></div></div></td>
      <td>${esc(flSource(f))}</td>
      <td class="num">${esc(flDur(f.ms))}</td>
      <td class="fl-acts">${taskBtn}<button class="small ghost" data-fl-copy="${f.id}">${esc(t("fl.copy"))}</button></td>
    </tr>`;
  }).join("");
}
listen("flash", (e) => {
  if (!flashes) return;
  flashes.rows.unshift(e.payload);
  if (currentTab === "flashes") renderFlashes();
});
$("#fl-body").addEventListener("click", async (e) => {
  const el = e.target.closest("button");
  if (!el) return;
  if (el.dataset.flCopy) {
    const f = flashes.rows.find((x) => String(x.id) === el.dataset.flCopy);
    try { await navigator.clipboard.writeText(f?.cmd || ""); toast(t("fl.copied"), "ok"); } catch {}
    return;
  }
  const path = el.dataset.flTask;
  if (!path) return;
  const name = path.split("\\").pop();
  if (!(await modal(t("st.offTitle", { name }), `<p>${esc(t("st.offBody.task"))}</p><p class="sub">${esc(t("st.backup"))}</p>`, { okText: t("st.off").replace("…", "") }))) return;
  busy(el, true);
  try {
    const id = await call("add_startup_tweak", { kind: "task", key: path, display: name });
    const r = await call("apply_tweaks", { ids: [id] });
    const res = r.results[0];
    toast(res?.ok ? t("st.done", { name }) : t("proc.problem", { m: res?.messages.join("; ") }), res?.ok ? "ok" : "err", 8000);
    selected.add(id);
  } catch { busy(el, false); return; }
  await loadState();
  renderFlashes();
});
$("#btn-fl-clear").onclick = async () => {
  if (!(await modal(t("fl.clearTitle"), "", { okText: t("fl.clear") }))) return;
  try { await call("clear_flashes"); flashes.rows = []; renderFlashes(); toast(t("fl.cleared"), "ok"); } catch {}
};

// ---------- dashboard ----------
let dashLoading = false;
async function loadDashboard() {
  if (dashLoading) return;
  dashLoading = true;
  try { await renderDashboard(); } finally { dashLoading = false; }
}

// the watchdog logs a "checked, nothing drifted" row every few minutes: noise on the dashboard
const quietRow = (h) => h.action === "reapply" && h.result === "ok" && /drifted 0\b/.test(h.detail || "");

function feedItem(h) {
  const cls = h.result !== "ok" ? "err" : h.action === "drift" || h.action === "reapply" ? "info" : "";
  const name = tweakName(h.tweak_id);
  return `<li title="${esc(h.detail)}"><i class="dot ${cls}"></i><div class="what"><b>${esc(actLabel(h))}</b>${name ? ` · ${esc(name)}` : ""}
    <div class="sub">${agoHtml(h.ts)} · ${esc(tOr("src." + h.source, h.source))}</div></div></li>`;
}

// ---------- impact: what RigSweep has freed on this PC ----------
let impact = null;
let impactPeriod = ["week", "month", "all"].includes(safeGet("impact_period")) ? safeGet("impact_period") : "month";
const IM_TAB = { memory: "cleanup", disk: "disk", browser: "browser" };
function impactLabel(k) {
  if (k.kind === "earlier") return t("impact.k.earlier");
  if (k.area === "memory") return tOr("impact.k." + k.kind, k.kind);
  if (k.kind === "worktree") return t("impact.k.worktree");
  if (k.kind === "dir:other") return t("impact.k.dirOther");
  if (k.kind.startsWith("dir:")) return k.kind.slice(4); // a folder name reads the same in every language
  if (k.kind.startsWith("cache:")) return tOr("disk.cache." + k.kind.slice(6), k.kind.slice(6));
  if (k.kind.startsWith("browser:")) return tOr("bc.item." + k.kind.slice(8), k.kind.slice(8));
  return k.kind;
}
async function loadImpact() {
  try { impact = await invoke("get_impact"); } catch (e) { console.error("get_impact", e); return; }
  renderImpact();
  impactMilestone();
}
function renderImpact() {
  document.querySelectorAll("#im-period button").forEach((b) => b.setAttribute("aria-checked", String(b.dataset.period === impactPeriod)));
  $("#im-period").setAttribute("aria-label", t("impact.title"));
  syncRadios($("#im-period"));
  if (!impact) return;
  const p = impact[impactPeriod];
  $("#im-disk").textContent = fmtBytes(p.disk + p.browser);
  $("#im-ram").textContent = fmtBytes(p.memory);
  $("#im-procs").textContent = ltr(String(p.processes));
  $("#im-ran").textContent = p.run_secs >= 60 ? t("impact.ranFor", { d: fmtDur(p.run_secs) }) : "";
  $("#im-cpu").textContent = p.cpu_peak >= 0.5 ? `${p.cpu_peak.toFixed(0)}%` : "–";
  $("#im-cputime").textContent = p.cpu_secs >= 1 ? fmtDur(p.cpu_secs) : "–";
  $("#im-runs").textContent = ltr(String(p.cleanups));
  const top = p.kinds.filter((k) => k.bytes > 0).slice(0, 10);
  const max = top[0]?.bytes || 1;
  $("#im-top").innerHTML = top.length ? top.map((k) => `<li><button class="im-row ${esc(k.area)}" data-go="${IM_TAB[k.area] || "dashboard"}">
      <span class="name" title="${esc(impactLabel(k))}"><bdi>${esc(impactLabel(k))}</bdi></span>
      <span class="bar" aria-hidden="true"><i style="width:${Math.max(2, (k.bytes / max) * 100)}%"></i></span>
      <span class="num">${fmtBytes(k.bytes)}</span></button></li>`).join("")
    : `<li class="empty-state">${esc(t(impact.all.cleanups ? "impact.emptyPeriod" : "impact.empty"))}</li>`;
  $("#im-since").textContent = impact.since ? t("impact.since", { d: parseTs(impact.since)?.toLocaleDateString(LANG, { dateStyle: "medium" }) ?? impact.since.slice(0, 10) }) : "";
}
// a toast the first time the all-time total passes each of these (in GB)
const IM_MILESTONES = [10, 50, 100, 250, 500, 1000];
function impactMilestone() {
  const gb = (impact.all.disk + impact.all.browser) / 1024 ** 3;
  const hit = IM_MILESTONES.filter((m) => gb >= m).at(-1);
  if (!hit || hit <= Number(safeGet("impact_ms") || 0)) return;
  safeSet("impact_ms", String(hit));
  toast(t("impact.milestone", { n: fmtBytes(hit * 1024 ** 3) }), "ok", 8000);
}
$("#im-period").addEventListener("click", (e) => {
  const b = e.target.closest("button[data-period]");
  if (!b) return;
  impactPeriod = b.dataset.period;
  safeSet("impact_period", impactPeriod);
  renderImpact();
});
$("#im-top").addEventListener("click", (e) => { const b = e.target.closest("[data-go]"); if (b) showTab(b.dataset.go); });

async function renderDashboard() {
  loadImpact();
  const [snap, hist, devr] = await Promise.all([
    invoke("get_processes").catch(() => null),
    invoke("get_history", { limit: 60 }).catch(() => []),
    invoke("get_dev_processes").catch(() => null),
  ]);
  if (snap) {
    snapshot = snap;
    $("#t-procs").textContent = ltr(`${snap.process_count} / ${snap.service_running}`);
    // Chrome runs as many processes; when together they hold a lot of RAM, show them as one row
    // (with the tab-helper link) instead of a callout above the page
    const chrome = snap.processes.filter((p) => p.name.toLowerCase() === "chrome.exe");
    const cmem = chrome.reduce((n, p) => n + p.mem, 0);
    const groupChrome = cmem > 1.5 * 1024 ** 3;
    const rank = (p) => p.cpu + diskOf(p) / 5e6;
    const group = groupChrome && { ...chrome.reduce((x, y) => (y.mem > x.mem ? y : x)), group: chrome.length, services: [], mem: cmem,
      cpu: chrome.reduce((n, p) => n + p.cpu, 0), disk_read: chrome.reduce((n, p) => n + (p.disk_read || 0), 0), disk_write: chrome.reduce((n, p) => n + (p.disk_write || 0), 0) };
    const top = snap.processes.filter((p) => !groupChrome || p.name.toLowerCase() !== "chrome.exe")
      .sort((x, y) => rank(y) - rank(x)).slice(0, 8);
    // the Chrome row is there for its RAM, so it always shows, in its CPU place or last
    if (group) { const i = top.findIndex((p) => rank(p) < rank(group)); i < 0 ? top.splice(7, 1, group) : (top.splice(i, 0, group), top.length = Math.min(top.length, 8)); }
    // a busy process gets its "See why" right next to its CPU; Defender is worth asking about sooner
    const whyBtn = (p) => {
      const defender = p.name.toLowerCase() === "msmpeng.exe";
      return !p.group && (p.cpu > 25 || (defender && p.cpu > 5))
        ? ` <button class="small ghost" data-why="${p.pid}">${esc(t("mp.why"))}</button>` : "";
    };
    const row = (p) => `<tr><td><div class="named">${ico(p.exe)}<div>${p.group
        ? `<b>${esc(p.name)}</b><div class="sub">${esc(t("dash.chrome", { n: p.group, mem: fmtBytes(p.mem) }))}</div>`
        : `<button class="linkish" data-why="${p.pid}" title="${esc(t("why.btn"))}"><b>${esc(p.name)}</b></button>${p.services.length ? `<div class="sub">${esc(p.services.map((s) => s.name).join(", "))}</div>` : ""}`}</div></div></td>
      <td class="num ${p.cpu > 25 ? "vhot" : p.cpu > 5 ? "hot" : ""}"><span class="cell-act">${whyBtn(p)}<span>${p.cpu.toFixed(1)}%</span></span></td>
      <td class="num"><span class="cell-act">${p.group ? `<button class="small ghost" data-go="settings" data-stab-go="about">${esc(t("dash.chrome.go"))}</button>` : ""}<span>${fmtBytes(p.mem)}</span></span></td>
      <td class="num">${diskOf(p) ? fmtBytes(diskOf(p)) : "–"}</td></tr>`;
    $("#dash-top").innerHTML = top.map(row).join("");
  } else {
    $("#dash-top").innerHTML = `<tr><td colspan="4" class="loading">${esc(t("err.noData"))}</td></tr>`;
  }
  if (devr) dev = devr;
  renderReady(); // stale processes now; disk and browser caches from the background scans
  // drop the loading skeletons even if a call failed; "–" then stands for "no data"
  ["#t-procs", "#t-tweaks"].forEach((id) => $(id).classList.remove("skel"));
  renderDashCharts();
  renderHeat();
  hardwareCallout();
  if (state) {
    // tweaks for software that isn't on this PC (PC maker services, for example) don't count
    const here = state.tweaks.filter((x) => x.status.overall !== "not_present");
    const applied = here.filter((x) => x.status.overall === "applied").length;
    $("#t-tweaks").textContent = ltr(`${applied} / ${here.length}`);
    $("#t-tweaks-bar").style.width = `${here.length ? (applied / here.length) * 100 : 0}%`;
    // how often Windows undid a tweak (and RigSweep put it back): a footnote under the tweak count
    const drift = state.tweaks.reduce((n, x) => n + x.drift_count, 0);
    $("#t-drift").textContent = drift ? `${t("dash.drift")}: ${drift}` : "";
  }
  const rows = hist.filter((h) => !quietRow(h)).slice(0, 8);
  $("#dash-history").innerHTML = rows.length ? rows.map(feedItem).join("")
    : `<li class="empty-state">${esc(t("dash.empty"))}</li>`;
}

// CPU and RAM charts: the backend's 2-second samples, last 5 minutes, both on a 0–100 % scale
const memPct = (s) => (s.mem_total ? (s.mem_used / s.mem_total) * 100 : 0);
const CHARTS = {
  cpu: { val: (s) => Math.min(100, s.cpu), tip: (s) => `${s.cpu.toFixed(0)}%` },
  mem: { val: memPct, tip: (s) => `${memPct(s).toFixed(0)}% · ${fmtBytes(s.mem_used)}` },
};

function renderDashCharts() {
  const last = live.at(-1);
  if (last) {
    const cpus = live.map((s) => s.cpu);
    $("#t-cpu").textContent = `${last.cpu.toFixed(0)}%`;
    $("#t-cpu-sub").textContent = t("dash.cpuSub", { a: avg(cpus).toFixed(0), p: Math.max(...cpus).toFixed(0) });
    $("#t-mem").textContent = `${memPct(last).toFixed(0)}%`;
    $("#t-mem-sub").textContent = t("dash.memSub", { u: fmtBytes(last.mem_used), t: fmtBytes(last.mem_total) });
    $("#t-cpu").classList.remove("skel");
    $("#t-mem").classList.remove("skel");
  }
  Object.keys(CHARTS).forEach(drawChart);
}

// Live charts keep their SVG shapes and only get new coordinates on each sample: nothing is
// re-parsed or re-created, and an attribute that didn't change isn't touched.
const SVG_NS = "http://www.w3.org/2000/svg";
function shape(svg, key, tag, cls) {
  let el = svg.querySelector(`:scope > [data-k="${key}"]`);
  if (!el) { el = document.createElementNS(SVG_NS, tag); el.dataset.k = key; svg.append(el); }
  if (cls != null && el.getAttribute("class") !== cls) el.setAttribute("class", cls);
  return el;
}
function setAttrs(el, attrs) {
  for (const [k, v] of Object.entries(attrs)) if (el.getAttribute(k) !== String(v)) el.setAttribute(k, v);
  return el;
}

function drawChart(id) {
  const plot = $(`#p-${id}`), svg = plot.querySelector("svg"), c = CHARTS[id];
  const enough = live.length >= 2;
  plot.querySelector(".chart-empty").classList.toggle("hidden", enough);
  if (!enough) { svg.replaceChildren(); chartHover(id); return; }
  const W = 300, H = 100, t0 = live[0].t, span = Math.max(1, live.at(-1).t - t0);
  const pts = live.map((s) => `${(((s.t - t0) / span) * W).toFixed(1)},${(H - c.val(s)).toFixed(1)}`).join(" ");
  setAttrs(shape(svg, "g1", "line", "grid"), { x1: 0, x2: W, y1: 50, y2: 50 });
  setAttrs(shape(svg, "g2", "line", "grid"), { x1: 0, x2: W, y1: 0.5, y2: 0.5 });
  setAttrs(shape(svg, "area", "polygon", "area"), { points: `0,${H} ${pts} ${W},${H}` });
  setAttrs(shape(svg, "line", "polyline", "line"), { points: pts });
  chartHover(id);
}

// crosshair + tooltip on the sample nearest the pointer
function chartHover(id) {
  const plot = $(`#p-${id}`), f = plot._hover, c = CHARTS[id];
  const [cur, dot, tip] = [".chart-cursor", ".chart-dot", ".chart-tip"].map((q) => plot.querySelector(q));
  const off = f == null || live.length < 2;
  [cur, dot, tip].forEach((e) => e.classList.toggle("hidden", off));
  if (off) return;
  const t0 = live[0].t, span = Math.max(1, live.at(-1).t - t0), at = t0 + f * span;
  const s = live.reduce((b, x) => (Math.abs(x.t - at) < Math.abs(b.t - at) ? x : b));
  const x = ((s.t - t0) / span) * 100;
  cur.style.left = dot.style.left = `${x}%`;
  dot.style.top = `${100 - c.val(s)}%`;
  tip.style.left = `${Math.min(85, Math.max(15, x))}%`;
  const when = new Date(s.t).toLocaleTimeString(LANG, { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  tip.innerHTML = `<b>${esc(c.tip(s))}</b><span class="sub">${esc(when)}</span>`;
}

Object.keys(CHARTS).forEach((id) => {
  const plot = $(`#p-${id}`);
  plot.addEventListener("pointermove", (e) => {
    const r = plot.getBoundingClientRect();
    plot._hover = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
    chartHover(id);
  });
  plot.addEventListener("pointerleave", () => { plot._hover = null; chartHover(id); });
});

// a tile links to its page only while that page is shown (Simple mode hides Processes, for one)
function syncTileLinks() {
  document.querySelectorAll(".tile[data-go]").forEach((el) => {
    const on = pageOn(el.dataset.go);
    el.classList.toggle("link", on);
    if (on) { el.setAttribute("role", "button"); el.tabIndex = 0; } else { el.removeAttribute("role"); el.removeAttribute("tabindex"); }
  });
}
document.querySelectorAll(".tile[data-go]").forEach((el) => {
  const go = () => { if (pageOn(el.dataset.go)) showTab(el.dataset.go); };
  el.onclick = go;
  el.onkeydown = (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); go(); } };
});
syncTileLinks();

$("#btn-refresh-dash").onclick = async (e) => {
  busy(e.target.closest("button"), true);
  try { await loadState(); await loadDashboard(); } finally { busy(e.target.closest("button"), false); }
};
$("#dash-top").addEventListener("click", (e) => {
  const b = e.target.closest("[data-go]");
  if (!b) return;
  showTab(b.dataset.go);
  if (b.dataset.stabGo) showSettingsTab(b.dataset.stabGo);
});
$("#hw-callout").addEventListener("click", (e) => { if (e.target.dataset.go) showTab(e.target.dataset.go); });

// ---------- hardware ----------
let hw = null;              // hardware inventory (the backend caches it until Refresh)
const bench = {};           // drive letter -> speed test result, or "run" while it runs
const RAM_TYPES = { 20: "DDR", 21: "DDR2", 24: "DDR3", 26: "DDR4", 27: "LPDDR", 28: "LPDDR2", 29: "LPDDR3", 30: "LPDDR4", 34: "DDR5", 35: "LPDDR5" };
const kv = (rows) => `<dl class="kv">${rows.filter(([, v]) => v != null && v !== "").map(([k, v]) => `<dt>${esc(t(k))}</dt><dd>${v}</dd>`).join("")}</dl>`;
const nz = (v) => (v == null || v === 0 ? null : v);

// ok | warn | bad
function diskHealth(d) {
  const errs = (d.read_err || 0) + (d.write_err || 0);
  if ((d.health && d.health !== "Healthy") || errs > 0) return "bad";
  if ((d.wear ?? 0) >= 80 || (d.temp ?? 0) >= 70) return "warn";
  return "ok";
}

async function loadHardware(fresh = false) {
  const btn = $("#btn-hw-refresh");
  busy(btn, true);
  try { hw = await call("get_hardware", { fresh }); } catch { hw = hw || {}; } finally { busy(btn, false); }
  renderHardware();
}
$("#btn-hw-refresh").onclick = () => loadHardware(true);

// identical sticks collapse into one "8 × 4 GB" line
function groupModules(ram) {
  const g = new Map();
  for (const m of ram) {
    const k = [m.bytes, m.maker, m.part].join("|");
    g.set(k, { m, n: (g.get(k)?.n || 0) + 1 });
  }
  return [...g.values()].map(({ m, n }) => `${n > 1 ? `${n} × ` : ""}${fmtBytes(m.bytes)} <span class="sub">${esc([m.maker, m.part, n === 1 && m.slot].filter(Boolean).join(" · "))}</span>`).join("<br>");
}

function renderHardware() {
  if (!hw) return;
  const last = live.at(-1);
  const disks = hw.disks || [];
  const running = Object.values(bench).includes("run");
  $("#hw-disks").innerHTML = disks.length ? disks.map((d) => {
    const h = diskHealth(d);
    const smart = d.temp != null || d.wear != null || d.hours != null;
    const wear = d.wear != null ? `${100 - d.wear}% <div class="meter ${h === "ok" ? "" : h}"><i style="width:${100 - d.wear}%"></i></div>` : null;
    const benches = (d.letters || []).map((l) => {
      const b = bench[l];
      if (b === "run") return `<div class="bench sub">${esc(t("hw.speed.running", { d: l }))}</div>`;
      if (b) return `<div class="bench"><b>${esc(l)}</b> ${esc(t("hw.speed.result", { r: b.seq_read.toFixed(0), w: b.seq_write.toFixed(0), k: b.rnd_read.toFixed(1), iops: b.rnd_iops.toFixed(0) }))}</div>`;
      return "";
    }).join("");
    return `<div class="card">
      <h2><span>${d.media === "SSD" ? "⚡" : d.media === "HDD" ? "💿" : "💽"} ${esc(d.model || "Disk " + d.id)}</span><span class="health ${h}">${esc(t("hw.health." + h))}</span></h2>
      ${kv([
        ["hw.size", fmtBytes(d.bytes)],
        ["hw.type", esc([d.media !== "Unspecified" ? d.media : "", d.bus].filter(Boolean).join(" · "))],
        ["hw.drives", esc((d.letters || []).join(" "))],
        ["hw.life", wear],
        ["heat.title", nz(d.temp) != null ? `${fmtTemp(d.temp)}${nz(d.temp_max) ? ` <span class="sub">(${esc(t("hw.max"))} ${fmtTemp(d.temp_max)})</span>` : ""}` : null],
        ["hw.hours", d.hours != null ? esc(t("hw.hoursVal", { n: d.hours.toLocaleString(), d: Math.round(d.hours / 24).toLocaleString() })) : null],
        ["hw.errors", d.read_err != null || d.write_err != null ? String((d.read_err || 0) + (d.write_err || 0)) : null],
        ["hw.status", d.status && d.status !== "OK" ? esc(d.status) : null],
        ["hw.firmware", esc(d.firmware)],
      ])}
      ${smart ? "" : `<p class="sub">${esc(t("hw.noSmart"))}</p>`}
      ${h === "bad" ? `<p class="warn-text">${esc(t("hw.backup"))}</p>` : ""}
      ${benches}
      <div class="hw-actions">${(d.letters || []).map((l) => `<button class="small ghost" data-bench="${esc(l)}" ${running ? "disabled" : ""}>${esc(t("hw.speed", { d: l }))}</button>`).join("")}</div>
    </div>`;
  }).join("") : `<div class="card sub">${esc(t("hw.none"))}</div>`;

  const cards = [];
  const cd = hw.cpu_detail || {};
  // CPU-Z style detail (cpuid): only on the first CPU card
  const cacheLine = (cd.caches || []).map((k) => `L${k.level}${k.kind === "data" ? "d" : k.kind === "instruction" ? "i" : ""} ${k.count > 1 ? `${k.count}× ` : ""}${fmtBytes(k.size_kb * 1024)}${k.ways ? ` · ${k.ways}-way` : ""}`).join("<br>");
  for (const [i, c] of (hw.cpu || []).entries()) cards.push(`<div class="card"><h2><span>🧮 ${esc(t("hw.cpu"))}</span></h2>${kv([
    ["hw.model", esc(c.name)],
    ["hw.codename", i === 0 && cd.codename ? esc([cd.codename, cd.uarch].filter(Boolean).join(" · ")) : null],
    ["hw.cpuid", i === 0 && cd.family != null ? esc(`${cd.vendor || ""} · family ${cd.family}, model ${cd.model}, stepping ${cd.stepping}`) : null],
    ["hw.hybrid", i === 0 && cd.hybrid ? esc(t("hw.hybridVal", { p: cd.p_cores ?? "?", e: cd.e_cores ?? "?" })) : null],
    ["hw.curClock", i === 0 && nz(cd.cur_mhz) ? `${(cd.cur_mhz / 1000).toFixed(2)} GHz` : null],
    ["hw.cores", nz(c.cores) ? esc(t("hw.coresVal", { c: c.cores, t: c.threads ?? c.cores })) : null],
    ["hw.clock", nz(c.max_mhz) ? `${(c.max_mhz / 1000).toFixed(2)} GHz` : null],
    ["hw.cache", i === 0 && cacheLine ? `<span class="pro-only">${cacheLine}</span><span class="simple-only">${[nz(c.l2_kb) && `L2 ${fmtBytes(c.l2_kb * 1024)}`, nz(c.l3_kb) && `L3 ${fmtBytes(c.l3_kb * 1024)}`].filter(Boolean).join(" · ")}</span>`
      : [nz(c.l2_kb) && `L2 ${fmtBytes(c.l2_kb * 1024)}`, nz(c.l3_kb) && `L3 ${fmtBytes(c.l3_kb * 1024)}`].filter(Boolean).join(" · ")],
    ["hw.features", i === 0 && prefs.mode === "pro" && cd.features?.length ? `<span class="feat">${cd.features.map((f) => `<code>${esc(f)}</code>`).join(" ")}</span>` : null],
    ["hw.socket", esc(c.socket)],
    ["hw.virt", c.virt == null ? null : esc(t(c.virt ? "hw.on" : "hw.virtOff"))],
    ["heat.title", last?.cpu_temp != null ? `${fmtTemp(last.cpu_temp)} <span class="sub">${esc(t("heat.acpi"))}</span>` : null],
  ])}</div>`);
  for (const g of hw.gpu || []) cards.push(`<div class="card"><h2><span>🎮 ${esc(t("hw.gpu"))}</span></h2>${kv([
    ["hw.model", esc(g.name)],
    ["hw.vram", nz(g.vram) ? fmtBytes(g.vram) : null],
    ["hw.driver", esc([g.driver, g.driver_date && `(${g.driver_date})`].filter(Boolean).join(" "))],
    ["hw.display", nz(g.width) ? `${Number(g.width)}×${Number(g.height)}${nz(g.hz) ? ` @ ${Number(g.hz)} Hz` : ""}` : null],
    ["heat.title", last?.gpu_temp != null ? fmtTemp(last.gpu_temp) : null],
  ])}</div>`);
  const ram = hw.ram || [];
  if (ram.length) {
    const total = ram.reduce((a, m) => a + (m.bytes || 0), 0), m0 = ram[0];
    cards.push(`<div class="card"><h2><span>🧠 ${esc(t("hw.ram"))}</span></h2>${kv([
      ["hw.installed", fmtBytes(total)],
      ["hw.type", esc([m0.type_name || RAM_TYPES[m0.type] || "", m0.form].filter(Boolean).join(" · "))],
      ["hw.channels", hw.ram_channels ? esc(t("hw.channelsVal", { n: hw.ram_channels })) : null],
      ["hw.ramSpeed", nz(m0.mhz || m0.rated) ? `${Number(m0.mhz || m0.rated)} MT/s` : null],
      ["hw.slots", hw.ram_slots ? esc(t("hw.slotsVal", { u: ram.length, n: hw.ram_slots })) : null],
      ["hw.modules", groupModules(ram)],
    ])}</div>`);
  }
  const sys = hw.system || {}, bd = hw.board || {};
  const up = sys.boot ? (Date.now() - Date.parse(sys.boot)) / 1000 : null;
  cards.push(`<div class="card"><h2><span>💻 ${esc(t("hw.system"))}</span></h2>${kv([
    ["hw.model", esc([sys.maker, sys.model].filter(Boolean).join(" "))],
    ["hw.board", esc([bd.maker, bd.model].filter(Boolean).join(" "))],
    ["hw.bios", esc([bd.bios, bd.bios_date && `(${bd.bios_date})`].filter(Boolean).join(" "))],
    ["hw.windows", esc([sys.os, sys.build && `build ${sys.build}`].filter(Boolean).join(", "))],
    ["hw.uptime", up > 0 ? esc(fmtAge(up)) : null],
  ])}</div>`);
  for (const b of hw.battery || []) {
    if (!nz(b.full)) continue;
    const pct = nz(b.design) ? Math.min(100, Math.round((b.full / b.design) * 100)) : null;
    const cls = pct == null || pct >= 80 ? "" : pct >= 60 ? "warn" : "bad";
    cards.push(`<div class="card"><h2><span>🔋 ${esc(t("hw.battery"))}</span></h2>${kv([
      ["hw.batt.health", pct != null ? `${pct}% <div class="meter ${cls}"><i style="width:${pct}%"></i></div>` : null],
      ["hw.batt.full", `${(b.full / 1000).toFixed(1)} Wh`],
      ["hw.batt.design", nz(b.design) ? `${(b.design / 1000).toFixed(1)} Wh` : null],
      ["hw.batt.cycles", nz(b.cycles) ? String(b.cycles) : null],
      ["hw.batt.chem", esc(b.chemistry || "")],
    ])}</div>`);
  }
  $("#hw-parts").innerHTML = cards.join("");
}

$("#hw-disks").addEventListener("click", async (e) => {
  const d = e.target.closest("[data-bench]")?.dataset.bench;
  if (!d) return;
  if (!(await modal(t("hw.speed.title", { d }), `<p>${esc(t("hw.speed.note"))}</p>`, { okText: t("hw.speed.go") }))) return;
  bench[d] = "run";
  renderHardware();
  try { bench[d] = await call("disk_speed_test", { drive: d }); } catch { delete bench[d]; }
  renderHardware();
});

// a failing disk earns a dashboard warning; the inventory is read once, in the background
async function hardwareCallout() {
  try { hw = hw || await invoke("get_hardware", { fresh: false }); } catch { return; }
  const bad = (hw.disks || []).filter((d) => diskHealth(d) === "bad");
  $("#hw-callout").innerHTML = bad.map((d) => `<div class="callout hot">${esc(t("hw.diskCallout", { m: d.model, s: d.health }))} <button class="small" data-go="hardware">${esc(t("hw.go"))}</button></div>`).join("");
}

// ---------- history ----------
async function loadHistory() {
  const rows = await call("get_history", { limit: 500 });
  $("#backup-info").textContent = t("hist.info", { n: state?.backup_count ?? 0, dir: state?.data_dir ?? "" });
  $("#hist-body").innerHTML = rows.length ? rows.map((h) => `<tr>
    <td class="ts">${agoHtml(h.ts)}</td><td>${esc(tOr("src." + h.source, h.source))}</td><td title="${esc(h.tweak_id || "")}">${esc(tweakName(h.tweak_id))}</td><td>${esc(actLabel(h))}</td>
    <td><span class="badge ${h.result === "ok" ? "active" : "orphan"}">${esc(h.result)}</span></td>
    <td class="sub">${esc(h.detail)}</td></tr>`).join("")
    : `<tr><td colspan="6" class="loading">${esc(t("dash.empty"))}</td></tr>`;
}

async function restoreAll() {
  if (!(await modal(t("hist.restoreTitle"), `<p>${esc(t("hist.restoreBody"))}</p>`, { okText: t("hist.restoreAll"), danger: true }))) return;
  const r = await call("restore_all");
  const failed = r.filter((x) => !x.ok);
  toast(failed.length ? t("hist.restoredErr", { n: failed.length }) : t("hist.restored", { n: r.length }), failed.length ? "warn" : "ok");
  selected.clear();
  await loadState();
  loadHistory();
}
$("#btn-restore-all").onclick = restoreAll;

$("#btn-export").onclick = async () => {
  const path = await call("export_profile");
  toast(t("hist.exported", { p: path }), "ok", 8000);
};
$("#file-import").addEventListener("change", async (e) => {
  const f = e.target.files[0];
  e.target.value = "";
  if (!f) return;
  const ids = await call("import_profile", { json: await f.text() });
  await loadState();
  if (!ids.length) return toast(t("hist.importNone"));
  // Apply goes ahead (risky tweaks still get their own confirm); Review ticks them on Tweaks and applies nothing
  const now = await modal(t("hist.importTitle"), `<p>${esc(t("hist.importBody", { n: ids.length }))}</p>`, { okText: t("tw.apply"), cancelText: t("hist.reviewFirst") });
  selected = new Set(ids);
  showTab("tweaks");
  renderTweaks();
  if (now) applyIds(ids, $("#btn-apply-selected"));
});

// ---------- settings ----------
// Settings → Appearance & features: everything applies at once, no restart
function renderLook() {
  document.querySelectorAll('[name="p-mode"]').forEach((r) => { r.checked = r.value === prefs.mode; });
  document.querySelectorAll("[data-mon]").forEach((c) => { c.checked = !!prefs[c.dataset.mon]; });
}
$("[data-stab-panel=look]").addEventListener("change", async (e) => {
  const el = e.target;
  if (el.name === "p-mode") prefs.mode = el.value;
  else if (el.dataset.mon) prefs[el.dataset.mon] = el.checked;
  else return;
  safeSet("prefs", JSON.stringify(prefs));
  applyPrefs();
  GROUPS = visibleGroups();
  syncTileLinks();
  renderPageTabs();
  if (["mon_heat", "mon_disk_alerts", "mon_flashes"].includes(el.dataset.mon)) {
    try { await call("set_monitors", { heat: prefs.mon_heat, diskAlerts: prefs.mon_disk_alerts, flashes: prefs.mon_flashes }); } catch {} // call() already showed the error
  }
  flashSaved();
});

// a redraw after a save leaves alone the field the user is in: they may already be typing the next value
function setField(sel, prop, v) {
  const el = $(sel);
  if (el && el !== document.activeElement) el[prop] = v;
}
function renderSettings() {
  if (!state) return;
  renderLook();
  const s = state.settings;
  setField("#s-startup", "checked", s.start_with_windows);
  setField("#s-tray", "checked", s.stay_in_tray);
  setField("#s-watchdog", "value", s.watchdog_minutes);
  setField("#s-live", "value", s.live_refresh_secs ?? 5);
  setField("#s-idle", "value", s.idle_minutes ?? 120);
  setField("#s-autokill", "checked", !!s.auto_kill_stale);
  setField("#s-badge", "checked", s.badge_enabled ?? true);
  setField("#s-badge-warn", "value", s.badge_warn_gb ?? 1);
  setField("#s-badge-alert", "value", s.badge_alert_gb ?? 2);
  setField("#s-folders", "value", s.dev_folders.join("\n"));
  setField("#s-autoupdate", "checked", !!s.auto_update);
  setField("#s-disk-roots", "value", (s.disk_roots || []).join("\n"));
  setField("#s-disk-excludes", "value", (s.disk_excludes || []).join("\n"));
  setField("#s-disk-stale", "value", s.disk_stale_days ?? 30);
  setField("#s-disk-auto", "checked", !!s.disk_auto_clean);
  setField("#s-disk-auto-low", "value", s.disk_auto_low_gb ?? 0);
  renderUpdate();
  $("#s-info").textContent = t("set.info", { s: state.startup_registered ? t("set.registered") : t("set.notRegistered"), d: state.data_dir });
  loadAv();
  loadShare();
}

async function loadAv() {
  try { avState = await invoke("defender_self_status"); } catch { avState = null; }
  const st = $("#av-status"), btn = $("#btn-av");
  if (!avState) { st.textContent = ""; btn.style.display = "none"; return; }
  if (!avState.available) { st.textContent = t("av.na"); btn.style.display = "none"; return; }
  btn.style.display = "";
  st.innerHTML = `${esc(t("av.folder", { f: avState.folder }))}<br>${esc(avState.excluded ? t("av.on") : t("av.off"))}`;
  btn.textContent = avState.excluded ? t("av.disallow") : t("av.allow");
}

$("#btn-av").onclick = async (e) => {
  const allow = !avState?.excluded;
  if (allow && !(await modal(t("av.title"), t("av.confirm", { f: esc(avState.folder) }), { okText: t("av.allow") }))) return;
  busy(e.target.closest("button"), true);
  try { avState = await call("defender_self_allow", { allow }); } finally { busy(e.target.closest("button"), false); }
  loadAv();
};

$("#btn-chrome").onclick = async () => {
  const p = await call("open_chrome_helper");
  await modal(t("chrome.title"), `<p class="sub">${esc(t("chrome.opened", { p }))}</p>${t("chrome.steps")}`, { cancel: false });
};

// how often the live pages refresh (Settings → General); the Dashboard uses twice this
function liveMs() { return Math.min(60, Math.max(2, Number(state?.settings?.live_refresh_secs) || 5)) * 1000; }

// ---------- settings: tabs and autosave ----------
let settingsTab = safeGet("settingsTab") || "general";
function showSettingsTab(name) {
  if (!document.querySelector(`[data-stab-panel="${name}"]`)) name = "general";
  settingsTab = name;
  document.querySelectorAll("[data-stab]").forEach((b) => {
    const on = b.dataset.stab === name;
    b.classList.toggle("active", on);
    b.setAttribute("aria-selected", on ? "true" : "false");
    b.tabIndex = on ? 0 : -1;
  });
  document.querySelectorAll("[data-stab-panel]").forEach((p) => { p.hidden = p.dataset.stabPanel !== name; });
  safeSet("settingsTab", name);
}
document.querySelectorAll("[data-stab]").forEach((b) => b.addEventListener("click", () => showSettingsTab(b.dataset.stab)));
$("#tab-settings .subtabs").addEventListener("keydown", (e) => {
  if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key)) return;
  const tabs = [...document.querySelectorAll("[data-stab]")];
  const i = tabs.findIndex((b) => b.dataset.stab === settingsTab);
  const rtl = document.documentElement.dir === "rtl";
  const step = { ArrowRight: rtl ? -1 : 1, ArrowLeft: rtl ? 1 : -1 }[e.key];
  const next = e.key === "Home" ? 0 : e.key === "End" ? tabs.length - 1 : (i + step + tabs.length) % tabs.length;
  e.preventDefault();
  showSettingsTab(tabs[next].dataset.stab);
  tabs[next].focus();
});
showSettingsTab(settingsTab);

// Every field in an .autosave card saves itself shortly after it changes; no Save button.
let saveTimer = null;
document.querySelectorAll(".autosave input, .autosave textarea").forEach((el) => {
  el.addEventListener("change", () => { clearTimeout(saveTimer); saveTimer = setTimeout(() => saveSettings().catch(() => {}), 300); });
});

// one save at a time: a change made while a save is in flight saves again right after it, with every field as it is then
let saving = false, saveAgain = false;
async function saveSettings() {
  if (saving) { saveAgain = true; return; }
  saving = true;
  try { await saveSettingsNow(); }
  finally {
    saving = false;
    if (saveAgain) { saveAgain = false; saveSettings().catch(() => {}); }
  }
}
async function saveSettingsNow() {
  const settings = {
    ...state.settings,
    start_with_windows: $("#s-startup").checked,
    stay_in_tray: $("#s-tray").checked,
    watchdog_minutes: Math.min(1440, Math.max(1, Math.round(Number($("#s-watchdog").value)) || 30)),
    live_refresh_secs: Math.min(60, Math.max(2, Math.round(Number($("#s-live").value)) || 5)),
    idle_minutes: Math.min(10080, Math.max(1, Math.round(Number($("#s-idle").value)) || 120)),
    auto_kill_stale: $("#s-autokill").checked,
    badge_enabled: $("#s-badge").checked,
    badge_warn_gb: Math.min(256, Math.max(0.5, Number($("#s-badge-warn").value) || 1)),
    badge_alert_gb: Math.min(256, Math.max(0.5, Number($("#s-badge-alert").value) || 2)),
    auto_update: $("#s-autoupdate").checked,
    dev_folders: lines($("#s-folders").value),
    disk_roots: lines($("#s-disk-roots").value),
    disk_excludes: lines($("#s-disk-excludes").value),
    disk_disabled: state.settings.disk_disabled || [],
    disk_stale_days: Math.min(3650, Math.max(1, Math.round(Number($("#s-disk-stale").value)) || 30)),
    disk_auto_clean: $("#s-disk-auto").checked,
    disk_auto_low_gb: Math.min(4096, Math.max(0, Math.round(Number($("#s-disk-auto-low").value)) || 0)),
  };
  const status = $("#set-saved");
  status.textContent = t("set.saving");
  try {
    const notes = await call("save_settings", { settings });
    status.textContent = t("set.saved");
    if (notes.length) toast(notes.join("\n"), "ok");
  } catch (err) {
    status.textContent = "";
    throw err;
  }
  await loadState();
  if (!saveAgain) renderSettings(); // another save is queued: it redraws once with the newest values
}

// ---------- updates ----------
// Nothing is checked unless the user clicks "Check now" or turns on automatic checks (off by
// default); the backend then emits "update-available". Installing always needs a click.
let update = null;      // {version, current, notes, date} when a newer version exists
let appVersion = "";
invoke("plugin:app|version").then((v) => { appVersion = v; renderUpdate(); }).catch(() => {});

function renderUpdate(status) {
  const pill = $("#update-pill");
  pill.classList.toggle("hidden", !update);
  if (update) pill.textContent = t("upd.pill", { v: update.version });
  const st = $("#upd-status"), notes = $("#upd-notes"), btn = $("#btn-upd-install");
  btn.classList.toggle("hidden", !update);
  notes.classList.toggle("hidden", !update?.notes);
  if (update?.notes) notes.innerHTML = `<b>${esc(t("upd.notes"))}</b><pre>${esc(update.notes)}</pre>`;
  if (status != null) st.textContent = status;
  else if (update) st.textContent = t("upd.available", { v: update.version, c: update.current });
  else if (appVersion) st.textContent = t("upd.current", { v: appVersion });
}

$("#btn-upd-check").onclick = async (e) => {
  busy(e.target.closest("button"), true, t("upd.checking"));
  try {
    update = await invoke("check_update");
    renderUpdate(update ? null : t("upd.latest", { v: appVersion }));
  } catch (err) {
    renderUpdate(t("upd.error", { m: String(err) }));
  } finally { busy(e.target.closest("button"), false); }
};

$("#btn-upd-install").onclick = async (e) => {
  if (!update) return;
  if (!(await modal(t("upd.confirm.title", { v: update.version }), `<p>${esc(t("upd.confirm.body"))}</p><p class="sub">${esc(t("upd.noReboot"))}</p>`, { okText: t("upd.install") }))) return;
  busy(e.target.closest("button"), true, t("upd.downloading", { p: "" }));
  $("#btn-upd-check").disabled = true;
  try { await invoke("install_update"); }  // on success RigSweep exits while the installer runs
  catch (err) { renderUpdate(t("upd.error", { m: String(err) })); busy(e.target.closest("button"), false); $("#btn-upd-check").disabled = false; }
};

$("#update-pill").onclick = () => showTab("settings");

listen("update-available", (e) => { update = e.payload; renderUpdate(); });
listen("update-progress", (e) => {
  const { done, total } = e.payload;
  const p = total ? `${Math.floor((done / total) * 100)} %` : fmtBytes(done);
  $("#btn-upd-install").textContent = t("upd.downloading", { p });
});

// ---------- live meter ----------
// The backend samples CPU / RAM every 2 s for as long as RigSweep runs. After a cleanup the
// recovered RAM is measured from those samples: RAM in use right before the kill minus the
// lowest reading since, so it shows up within seconds instead of only in the Boost Report.
let live = [];          // [{t, cpu, mem_used, mem_total, cpu_temp?, gpu_temp?}]
let recovery = null;    // {before, since, estimate, auto, until}
const RECOVERY_MS = 20000;
const avg = (a) => a.length ? a.reduce((n, x) => n + x, 0) / a.length : 0;

function startRecovery(before, since, estimate, auto) {
  const cpuBefore = avg(live.filter((s) => s.t < since).slice(-3).map((s) => s.cpu));
  recovery = { before, since, estimate, auto, cpuBefore, until: Date.now() + RECOVERY_MS };
  renderLive();
}

function recoveredNow() {
  if (!recovery?.before) return null;
  const after = live.filter((s) => s.t >= recovery.since);
  if (!after.length) return null;
  const best = Math.max(0, ...after.map((s) => recovery.before - s.mem_used));
  // the kill itself costs CPU: judge CPU on samples from 4 s after it
  const settled = after.filter((s) => s.t >= recovery.since + 4000).map((s) => s.cpu);
  return { best, cpuAfter: settled.length >= 2 ? avg(settled) : null };
}

function renderLive() {
  const last = live.at(-1);
  if (last) {
    $("#live-mem").textContent = `${fmtBytes(last.mem_used)} / ${fmtBytes(last.mem_total)}`;
    $("#live-cpu").textContent = `${last.cpu.toFixed(0)}%`;
    if (currentTab === "dashboard") renderDashCharts();
  }
  renderSpark();
  renderHeat();
  const win = $("#live-win");
  if (!recovery) { win.className = "live-win hidden"; return; }
  const r = recoveredNow();
  const measuring = Date.now() < recovery.until;
  const tag = recovery.auto ? ` <span class="sub">· ${esc(t("live.auto"))}</span>` : "";
  if (!r || (measuring && r.best < 32 * 1048576)) {
    win.className = "live-win pending";
    win.innerHTML = esc(t("live.recovering"));
    return;
  }
  win.className = "live-win";
  const cpu = r.cpuAfter != null && recovery.cpuBefore
    ? `<span class="sub">${esc(t("live.cpu", { a: recovery.cpuBefore.toFixed(0), b: r.cpuAfter.toFixed(0) }))}</span>` : "";
  const note = r.best < 32 * 1048576 && recovery.estimate
    ? `<span class="sub">${esc(t("live.estimate", { mem: fmtBytes(recovery.estimate) }))}</span>` : "";
  win.innerHTML = `<span class="big">${esc(t("live.recovered", { mem: fmtBytes(r.best) }))}</span>${cpu}${tag}${note ? "<br>" + note : ""}`;
}

// ---------- heat ----------
// CPU temperature comes from the ACPI thermal zone (approximate, often missing on desktops),
// GPU temperature from the same counter Task Manager uses. Either may be null.
const HEAT = { warm: 80, hot: 90 };
let heatF = (() => { const s = safeGet("heatUnit"); return s ? s === "F" : LANG === "en" && navigator.language === "en-US"; })();
const fmtTemp = (c) => ltr(heatF ? `${Math.round(c * 9 / 5 + 32)} °F` : `${Math.round(c)} °C`);
const heatCls = (c) => (c >= HEAT.hot ? "vhot" : c >= HEAT.warm ? "hot" : "");
const heatOf = (s) => Math.max(s.cpu_temp ?? -1, s.gpu_temp ?? -1);

function heatParts(s) {
  const parts = [];
  if (s.cpu_temp != null) parts.push(`<span dir="ltr" class="${heatCls(s.cpu_temp)}">${esc(t("heat.cpu"))} ${fmtTemp(s.cpu_temp)}</span>`);
  if (s.gpu_temp != null) parts.push(`<span dir="ltr" class="${heatCls(s.gpu_temp)}">${esc(t("heat.gpu"))} ${fmtTemp(s.gpu_temp)}</span>`);
  return parts.join(" · ");
}

function renderHeat() {
  const last = live.at(-1);
  const has = live.some((s) => s.cpu_temp != null || s.gpu_temp != null);
  $("#live-heat-row").classList.toggle("hidden", !has);
  if (last && has) $("#live-heat").innerHTML = heatParts(last) || "–";
  if (currentTab !== "dashboard") return;
  const tile = $("#t-heat-tile");
  if (!last) return;
  $("#t-heat").classList.remove("skel");
  if (!has) {
    $("#t-heat").textContent = t("heat.na");
    $("#t-heat-sub").textContent = t("heat.naHint");
    $("#heat-spark").replaceChildren();
    $("#heat-callout").innerHTML = "";
    tile.classList.add("na");
    return;
  }
  tile.classList.remove("na");
  $("#t-heat").innerHTML = heatParts(last) || "–";
  const peak = Math.max(...live.map(heatOf));
  $("#t-heat-sub").textContent = t("heat.peak", { t: fmtTemp(peak) }) + (last.cpu_temp != null ? " · " + t("heat.acpi") : "");
  // sparkline of the hotter sensor, scaled 30–100 °C so the warm/hot bands sit in the same place
  const pts = live.filter((s) => heatOf(s) > 0);
  if (pts.length >= 2) {
    const W = 300, H = 28, t0 = pts[0].t, span = Math.max(1, pts.at(-1).t - t0);
    const y = (c) => H - 1 - ((Math.min(100, Math.max(30, c)) - 30) / 70) * (H - 2);
    const line = pts.map((s) => `${(((s.t - t0) / span) * W).toFixed(1)},${y(heatOf(s)).toFixed(1)}`).join(" ");
    const svg = $("#heat-spark");
    setAttrs(shape(svg, "warn", "line", "warn"), { x1: 0, x2: W, y1: y(HEAT.warm), y2: y(HEAT.warm) });
    setAttrs(shape(svg, "line", "polyline", heatCls(peak) || "ok"), { points: line });
  }
  // warn only when it stays hot (last 3 samples), not on a single spike
  const recent = live.slice(-3);
  const hotNow = recent.length === 3 && recent.every((s) => heatOf(s) >= HEAT.hot);
  const which = last.cpu_temp != null && last.cpu_temp >= (last.gpu_temp ?? -1) ? t("heat.cpu") : t("heat.gpu");
  $("#heat-callout").innerHTML = hotNow
    ? `<div class="callout vhot">${esc(t("heat.hotCallout", { what: which, t: fmtTemp(heatOf(last)) }))} <button class="small" data-go="cleanup">${esc(t("dash.stale.go"))}</button></div>` : "";
}
const toggleHeatUnit = () => { heatF = !heatF; safeSet("heatUnit", heatF ? "F" : "C"); renderHeat(); };
$("#t-heat-tile").onclick = toggleHeatUnit;
$("#t-heat-tile").onkeydown = (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); toggleHeatUnit(); } };
$("#heat-callout").addEventListener("click", (e) => { if (e.target.dataset.go) showTab(e.target.dataset.go); });

// ---------- "Ready to free": what the scans found, with one-click actions ----------
// The last cleanup per tile ({ bytes, at }), so an empty tile can say "Freed 1 GB 5 minutes ago"
// for a while instead of just "nothing to free". Kept per viewer in localStorage.
const FREED_FOR = 24 * 3600e3;
let lastFreed = (() => { try { return JSON.parse(safeGet("lastFreed") || "{}") || {}; } catch { return {}; } })();
function noteFreed(key, bytes) {
  if (!(bytes > 0)) return;
  lastFreed[key] = { bytes, at: new Date().toISOString() };
  safeSet("lastFreed", JSON.stringify(lastFreed));
  if (typeof renderReady === "function") renderReady();
}
const recentFreed = (key) => { const f = lastFreed[key]; return f && Date.now() - (parseTs(f.at)?.getTime() ?? 0) < FREED_FOR ? f : null; };
// Dev Disk and the browser caches are scanned quietly in the background (low priority, every few
// hours), so the Dashboard can say what can be freed without the user opening those pages. Every
// action still goes through the same confirmation dialog as on its own page.
function readyRows() {
  const rows = [];
  const stale = (dev?.items || []).filter((p) => p.stale && !p.protected);
  if (stale.length) {
    const mem = stale.reduce((a, p) => a + p.tree_mem, 0), cpu = stale.reduce((a, p) => a + p.cpu, 0);
    rows.push({ key: "dev", icon: "🧹", go: "cleanup", act: t("ready.dev.act"), big: fmtBytes(mem),
      cap: t("ready.cap.dev", { n: stale.length }) + (cpu >= 1 ? " · " + t("ready.cpu", { c: cpu.toFixed(0) }) : ""),
      text: t("ready.dev", { n: stale.length, mem: fmtBytes(mem) }) });
  }
  const dsk = disk ? diskRows().filter(diskSuggested) : [];
  if (dsk.length) {
    const size = fmtBytes(dsk.reduce((a, r) => a + r.bytes, 0));
    rows.push({ key: "disk", icon: "💾", go: "disk", act: t("ready.clean"), big: size, cap: t("ready.cap.disk", { n: dsk.length }),
      text: t("ready.disk", { size, n: dsk.length }) });
  }
  const br = bc ? bcRows().filter(bcPreselect) : [];
  if (br.length) {
    const size = fmtBytes(br.reduce((a, r) => a + r.i.bytes, 0));
    rows.push({ key: "bc", icon: "🌐", go: "browser", act: t("ready.clean"), big: size, cap: t("ready.cap.bc"), text: t("ready.bc", { size }) });
  }
  return rows;
}
// nothing left to free, but this tile cleaned something recently: say how much and when
function freedTile(key, icon, go, f) {
  const size = fmtBytes(f.bytes), MARK = "\u0001";
  return { key, icon, go, state: "none", big: "✓", text: t("ready.freed", { size, ago: timeAgo(f.at, 60) }),
    capHtml: esc(t("ready.freed", { size, ago: MARK })).replace(MARK, agoHtml(f.at, 60)) };
}
function renderReady() {
  const have = readyRows();
  // always the same three tiles: checking, nothing to free, or the amount, so nothing jumps around
  const SLOTS = [["dev", "🧹", "cleanup", () => dev, "cleanup"], ["disk", "💾", "disk", () => disk, "disk"], ["bc", "🌐", "browser", () => bc, "browser"]];
  const rows = SLOTS.map(([key, icon, go, data, scan]) => have.find((r) => r.key === key)
    || (!data() && scanFailed[scan] ? { key, icon, go, state: "failed", big: "!", cap: t("err.failed"), text: t("err.failed") } : null)
    || (data() && recentFreed(key) ? freedTile(key, icon, go, recentFreed(key)) : null)
    || { key, icon, go, state: data() ? "none" : "checking", big: data() ? "✓" : "", cap: t(data() ? `ready.none.${key}` : "ready.scanHint"), text: t(data() ? `ready.none.${key}` : "ready.scanning") });
  // one tile per kind: the amount big, what it is underneath, the actions at the bottom
  renderDashSpace();
  $("#ready-list").innerHTML = rows.map((r) => `<li class="ready-tile ${r.key} ${r.state || "has"}" aria-label="${esc(r.text)}" ${r.state === "checking" ? 'aria-busy="true"' : ""}>
    <div class="ready-top"><span class="ready-ico" aria-hidden="true">${r.icon}</span><span class="ready-big">${r.state === "checking" ? `<span class="scanning"><i class="scan-dot"></i>${esc(t("ready.scanning"))}</span>` : esc(r.big)}</span></div>
    <div class="ready-cap"${r.capHtml ? ` title="${esc(t(`ready.none.${r.key}`))}"` : ""}>${r.capHtml ?? esc(r.cap)}</div>
    <div class="ready-acts"><button class="small ghost" data-go="${r.go}">${esc(t("ready.review"))}</button>${r.state ? "" : `<button class="small primary" data-ready="${r.key}">${esc(r.act)}</button>`}</div></li>`).join("");
}
// the whole PC on the Dashboard: what Dev Disk and Browser cleanup would free together, one
// button that goes through each page's own confirmation in turn
const dashCta = {
  disabled: false,
  click: async () => {
    const dsk = disk && !jobActive(diskJob) ? diskRows().filter(diskSuggested) : [];
    if (dsk.length) { diskSel = new Set(dsk.map((r) => r.k)); await $("#btn-disk-clean").onclick({ target: $("#btn-disk-clean") }); }
    const br = bc ? bcRows().filter(bcPreselect) : [];
    if (br.length) { bcSel = new Set(br.map((r) => r.k)); await $("#btn-bc-clean").onclick({ target: $("#btn-bc-clean") }); }
  },
};
function renderDashSpace() {
  const dRows = disk ? diskRows().filter((r) => !r.w?.orphan) : [], bRows = bc ? bcRows().filter((r) => !r.b.running) : [];
  const add = (a, b) => { b.forEach((v, k) => a.set(k, (a.get(k) || 0) + v)); return a; };
  const picked = add(sumBy(dRows.filter(diskSuggested), diskDrive, (r) => r.bytes), sumBy(bRows.filter(bcPreselect), sysDrive, (r) => r.i.bytes));
  const all = add(sumBy(dRows, diskDrive, (r) => r.bytes), sumBy(bRows, sysDrive, (r) => r.i.bytes));
  dashCta.disabled = jobActive(diskJob);
  if (!volumes) return loadVolumes().then(() => volumes && renderDashSpace());
  renderSpace($("#dash-space"), picked, all, dashCta);
}
$("#ready-list").addEventListener("click", (e) => {
  const go = e.target.closest("[data-go]");
  if (go) return showTab(go.dataset.go);
  const b = e.target.closest("[data-ready]");
  if (!b) return;
  // pick exactly what the row describes, then use the page's own (confirmed) action
  if (b.dataset.ready === "dev") {
    (dev?.items || []).filter((p) => p.stale && !p.protected).forEach((p) => { devSel.add(devKey(p)); devUnticked.delete(devKey(p)); });
    $("#btn-clean-end").click();
  } else if (b.dataset.ready === "disk") {
    // a clean-up already running: say so, and keep the selection the user made on Dev Disk
    if (jobActive(diskJob)) return toast(t("disk.job.busy"), "warn");
    diskSel = new Set(diskRows().filter(diskSuggested).map((r) => r.k));
    $("#btn-disk-clean").click();
  } else if (b.dataset.ready === "bc") {
    bcSel = new Set(bcRows().filter(bcPreselect).map((r) => r.k));
    $("#btn-bc-clean").click();
  }
});
const BACKGROUND = {
  disk: { every: 6 * 3600e3, run: () => scanDisk(), skip: () => jobActive(diskJob) },
  browser: { every: 2 * 3600e3, run: () => scanBrowsers() },
};
function backgroundScans() {
  for (const [key, b] of Object.entries(BACKGROUND)) {
    if (scans[key] || b.skip?.() || Date.now() - (scannedAt[key] || 0) < b.every) continue;
    scanOnce(key, b.run).then(renderReady, () => {});
  }
}
setTimeout(backgroundScans, 90e3);
setInterval(backgroundScans, 15 * 60e3);

// ---------- why is Microsoft Defender busy? ----------
// The backend records Defender for 20 s (its own performance recorder) and returns its report. Field
// names and units vary between Defender versions, so each row is read defensively and shown as a
// share of the scan time rather than in absolute units.
const MP_SECONDS = 20;
function mpRows(list) {
  const arr = Array.isArray(list) ? list : list ? [list] : [];
  const dur = (v) => typeof v === "number" ? v : v && typeof v === "object" ? Number(v.TotalMilliseconds ?? (v.Ticks != null ? v.Ticks / 1e4 : 0)) : Number(v) || 0;
  return arr.map((o) => {
    const name = ["Process", "ProcessPath", "Path", "Extension", "Name"].map((k) => o?.[k]).find((v) => typeof v === "string" && v)
      ?? Object.values(o || {}).find((v) => typeof v === "string") ?? "?";
    const key = Object.keys(o || {}).find((k) => /^totalduration$/i.test(k)) ?? Object.keys(o || {}).find((k) => /duration/i.test(k));
    return { name, dur: key ? dur(o[key]) : 0, count: Number(o?.Count ?? o?.ScanCount ?? 0) };
  }).filter((r) => r.name !== "?");
}
const DEV_HINT = /node_modules|[\\/](target|dist|build|obj|bin|\.next|\.git|\.venv|\.gradle|\.cargo|\.npm|\.pnpm-store)([\\/]|$)|^(node|cargo|rustc|msbuild|dotnet|java|javac|git|python|go|bun|deno|pnpm|npm)(\.exe)?$/i;
function mpList(title, rows) {
  if (!rows.length) return "";
  const total = rows.reduce((a, r) => a + r.dur, 0) || rows.reduce((a, r) => a + r.count, 0) || 1;
  return `<h4>${esc(title)}</h4><ul class="mp-list">${rows.map((r) => {
    const share = ((r.dur || r.count) / total) * 100;
    return `<li><span class="name"><bdi dir="ltr">${esc(r.name)}</bdi></span><span class="bar" aria-hidden="true"><i style="width:${Math.max(2, share).toFixed(0)}%"></i></span>
      <span class="num">${share.toFixed(0)}%${r.count ? ` · ${esc(t("mp.scans", { n: r.count }))}` : ""}</span></li>`;
  }).join("")}</ul>`;
}
// one recording at a time. The row button that started it is often redrawn away by the live table,
// so the progress also shows as a toast whose bar runs for the length of the recording.
let mpRunning = false;
async function defenderWhy(btn) {
  if (mpRunning) return; // its progress toast is already on screen
  mpRunning = true;
  busy(btn, true, t("mp.recording", { s: MP_SECONDS }));
  toast(t("mp.recording", { s: MP_SECONDS }), "info", MP_SECONDS * 1000);
  let r;
  try { r = await invoke("defender_why", { seconds: MP_SECONDS }); }
  catch (e) { return toast(t("mp.fail", { e: String(e) }), "err"); }
  finally { mpRunning = false; busy(btn, false); }
  const procs = mpRows(r?.processes), paths = mpRows(r?.paths), exts = mpRows(r?.extensions);
  const names = [...procs, ...paths].map((x) => x.name.split(/[\\/]/).pop() || x.name).concat(paths.map((x) => x.name));
  const dev = names.some((n) => DEV_HINT.test(n));
  const body = !procs.length && !paths.length && !exts.length ? `<p>${esc(t("mp.none"))}</p>`
    : mpList(t("mp.procs"), procs) + mpList(t("mp.paths"), paths) + mpList(t("mp.exts"), exts)
      + (dev ? `<p class="callout">${esc(t("mp.devHint"))}</p>` : "");
  const go = await modal(t("mp.title"), body, dev ? { okText: t("mp.devGo") } : { cancel: false });
  if (go && dev) showTab("tweaks");
}
document.addEventListener("click", (e) => { const b = e.target.closest("[data-why]"); if (b) processWhy(Number(b.dataset.why), b); });

// ---------- why is this process busy? ----------
// Plain-language reasons for processes that are often busy; everything else gets the facts only.
const WHY_REASON = [
  [/^msmpeng\.exe$/i, "defender"], [/^(searchindexer|searchprotocolhost|searchfilterhost)\.exe$/i, "search"],
  [/^(tiworker|trustedinstaller|mousocoreworker|wuauclt|usoclient)\.exe$/i, "update"], [/^svchost\.exe$/i, "svchost"],
  [/^wmiprvse\.exe$/i, "wmi"], [/^dwm\.exe$/i, "dwm"], [/^vmmem(wsl)?$/i, "vm"], [/^compattelrunner\.exe$/i, "telemetry"],
  [/^(node|bun|deno|python|pythonw|java|javaw|dotnet|ruby|php|go|cargo|rustc|gradle|msbuild)(\.exe)?$/i, "dev"],
  [/^(chrome|msedge|firefox|brave|opera|vivaldi)\.exe$/i, "browser"], [/^(code|cursor|windsurf|devenv|idea64|rider64)\.exe$/i, "editor"],
  [/^rigsweep\.exe$/i, "self"], [/^(system|registry|memory compression|secure system)$/i, "system"],
];
async function processWhy(pid, btn) {
  let d;
  busy(btn, true);
  try { d = await invoke("process_why", { pid }); }
  catch (e) { return toast(t("why.gone", { e: String(e) }), "warn"); }
  finally { busy(btn, false); }
  const reason = WHY_REASON.find(([re]) => re.test(d.name))?.[1];
  const row = (k, v, cls = "") => v ? `<div class="why-row"><span class="k">${esc(t(k))}</span><span class="v ${cls}">${v}</span></div>` : "";
  const chain = (list) => list.map((p) => `<li><button class="linkish" data-why="${p.pid}">${esc(p.name)}</button> <span class="sub">PID ${p.pid}${p.cpu >= 0.1 ? ` · ${p.cpu.toFixed(1)}%` : ""} · ${fmtBytes(p.mem)}</span></li>`).join("");
  const body = `${reason ? `<p class="why-reason">${esc(t("why.r." + reason))}</p>` : ""}
    <div class="why-facts">
      ${row("why.cpu", `${d.cpu.toFixed(1)}%`)}${row("why.mem", fmtBytes(d.mem))}
      ${row("why.disk", d.disk_read + d.disk_write ? `${fmtBytes(d.disk_read)}/s ↓ · ${fmtBytes(d.disk_write)}/s ↑` : "")}
      ${row("why.threads", d.threads ? String(d.threads) : "")}
      ${row("why.started", d.start_time ? esc(timeAgo(new Date(d.start_time * 1000).toISOString())) : "")}
      ${row("why.maker", esc(d.company))}${row("why.desc", esc(d.description))}
      ${row("why.path", `<bdi dir="ltr">${esc(d.exe)}</bdi>`, "mono")}${row("why.cmd", `<bdi dir="ltr">${esc(d.cmd)}</bdi>`, "mono")}
    </div>
    ${d.parents.length ? `<h4>${esc(t("why.parents"))}</h4><ul class="why-chain">${chain(d.parents)}</ul>` : ""}
    ${d.children.length ? `<h4>${esc(t("why.children"))}</h4><ul class="why-chain">${chain(d.children)}</ul>` : ""}`;
  const defender = reason === "defender";
  const ok = await modal(t("why.title", { name: d.name }), body, defender ? { okText: t("mp.record") } : { cancel: false });
  if (ok && defender) defenderWhy(btn);
}

function renderSpark() {
  const svg = $("#live-spark");
  if (live.length < 2) { svg.replaceChildren(); return; }
  const W = 300, H = 48, t0 = live[0].t, span = Math.max(1, live.at(-1).t - t0);
  // RAM is zoomed to its recent range (a 2 GB drop on a 32 GB machine would look flat otherwise)
  const mems = live.map((s) => s.mem_used);
  const lo0 = Math.min(...mems), hi0 = Math.max(...mems), pad = Math.max(hi0 - lo0, 512 * 1048576) * 0.35;
  const lo = Math.max(0, lo0 - pad), range = hi0 + pad - lo;
  const x = (s) => ((s.t - t0) / span) * W;
  const ym = (s) => H - 2 - ((s.mem_used - lo) / range) * (H - 4);
  const yc = (s) => H - 2 - (Math.min(100, s.cpu) / 100) * (H - 4);
  const mem = live.map((s) => `${x(s).toFixed(1)},${ym(s).toFixed(1)}`).join(" ");
  const cpu = live.map((s) => `${x(s).toFixed(1)},${yc(s).toFixed(1)}`).join(" ");
  setAttrs(shape(svg, "area", "polygon", "area"), { points: `0,${H} ${mem} ${W},${H}` });
  setAttrs(shape(svg, "cpu", "polyline", "cpu"), { points: cpu });
  setAttrs(shape(svg, "mem", "polyline", "mem"), { points: mem });
  const mark = shape(svg, "mark", "line", "mark");
  if (recovery && recovery.since >= t0) {
    const mx = (((recovery.since - t0) / span) * W).toFixed(1);
    setAttrs(mark, { x1: mx, x2: mx, y1: 0, y2: H, display: "inline" });
  } else setAttrs(mark, { display: "none" });
}

// Samples arrive every 2 s; they are always kept, but drawn only while someone can see them
// (window visible, on a page that shows them). Showing the window or the page draws them again.
const liveVisible = () => !document.hidden && LIVE_TABS.has(currentTab);
listen("live", (e) => {
  live.push(e.payload);
  if (live.length > 150) live.shift();
  if (liveVisible()) renderLive();
});
document.addEventListener("visibilitychange", () => { if (liveVisible()) renderLive(); });
invoke("get_live").then((h) => { if (h?.length) { live = h.concat(live.filter((s) => s.t > h.at(-1).t)); renderLive(); } }).catch(() => {});

// ---------- backend events ----------
listen("profile-changed", (e) => {
  const s = e.payload;
  if (s?.drifted) toast(t("tw.checked", { c: s.checked, d: s.drifted, f: s.fixed }));
  loadState();
});
listen("paused-changed", (e) => setPaused(e.payload));
listen("dev-autocleaned", (e) => {
  const r = e.payload;
  // the trees were ended a moment before this event; samples from just before it count
  if (r.mem_before) startRecovery(r.mem_before, Date.now() - 2500, r.freed_mem, true);
  noteFreed("dev", r.freed_mem);
  toast(t("clean.auto.done", { n: r.ended.length, mem: fmtBytes(r.freed_mem) }) + "\n• " + r.ended.join("\n• "), "ok", 8000);
  if (currentTab === "cleanup") loadDev(null, true); else delete scannedAt.cleanup;
});
// Settings > Dev Disk "clean automatically" ran (weekly, or the system drive got low)
listen("disk-autocleaned", async (e) => {
  noteFreed("disk", e.payload);
  await loadVolumes();
  toast(t("disk.auto.done", { mem: fmtBytes(e.payload) }) + spaceNow(), "ok", 9000);
  if (currentTab === "disk" || disk) loadDisk(true);
});
// background disk health check: a drive got worse (health, wear, errors, temperature)
listen("disk-alerts", (e) => {
  const lines = (e.payload || []).map((a) => `${a.model}: ${t("hw.alert." + a.kind, { from: a.from, to: a.to })}`);
  if (lines.length) toast(t("hw.alert.title") + "\n• " + lines.join("\n• "), e.payload.some((a) => a.severity === "bad") ? "error" : "info", 15000);
});
listen("request-restore-all", () => { showTab("history"); restoreAll(); });

// ---------- disclaimer ----------
async function ensureDisclaimer() {
  if (safeGet("disclaimer_v1") === "1") return;
  await modal(t("disc.title"), `<p>${esc(t("disc.body"))}</p>`, { okText: t("disc.accept"), cancel: false });
  safeSet("disclaimer_v1", "1");
}

// ---------- anonymous stats ----------
let shareState = null;

async function previewHtml() {
  let json = "";
  try { json = JSON.stringify((await invoke("get_report_preview")).report, null, 2); } catch (e) { json = String(e); }
  return `<pre class="share-pre">${esc(json)}</pre>`;
}

function renderShare() {
  const s = shareState;
  if (!s) return;
  $("#s-share").checked = s.enabled;
  // "Last sent: {d}" with {d} as a live time-ago element
  const status = !s.enabled ? esc(t("share.off")) : !s.last_sent ? esc(t("share.never"))
    : esc(t("share.last")).replace("{d}", () => agoHtml(s.last_sent));
  $("#share-status").innerHTML = [status, esc(t("share.id", { id: s.install_id }))].join("<br>");
}

async function loadShare() {
  try { shareState = await invoke("get_share_state"); } catch { shareState = null; }
  renderShare();
  return shareState;
}

$("#s-share").onchange = async (e) => {
  const on = e.target.checked;
  e.target.disabled = true;
  try {
    shareState = await call("set_share", { on });
    toast(t(on ? "share.onDone" : "share.offDone"), "ok");
  } catch { e.target.checked = !on; } finally { e.target.disabled = false; }
  renderShare();
};

$("#btn-share-preview").onclick = async (e) => {
  busy(e.target.closest("button"), true);
  let html;
  try { html = await previewHtml(); } finally { busy(e.target.closest("button"), false); }
  await modal(t("share.previewTitle"), html, { cancel: false });
};

$("#btn-share-reset").onclick = async (e) => {
  busy(e.target.closest("button"), true);
  try { shareState = await call("reset_install_id"); toast(t("share.resetDone"), "ok"); } finally { busy(e.target.closest("button"), false); }
  renderShare();
};

// Shown once (also to people updating from a version without sharing). Nothing is sent before it.
async function ensureShareNotice() {
  const s = await loadShare();
  if (!s || s.notice_seen) return;
  const optIn = s.consent_default === "opt_in";
  const body = `<p>${esc(t(optIn ? "share.notice.optin" : "share.notice.optout"))}</p>
    <p class="hint">${esc(t("share.hint"))}</p>
    <details class="share-details"><summary>${esc(t("share.preview"))}</summary>${await previewHtml()}</details>`;
  const yes = await modal(t("share.notice.title"), body, {
    okText: t(optIn ? "share.yes" : "share.keep"),
    cancelText: t(optIn ? "share.no" : "share.turnOff"),
  });
  try {
    shareState = await invoke(yes && !optIn ? "share_notice_seen" : "set_share", yes && !optIn ? undefined : { on: yes });
    if (yes) toast(t("share.onDone"), "ok");
  } catch (e) { console.error(e); }
  renderShare();
}

// ---------- boot ----------
(async () => {
  applyI18n();
  await loadState().catch(() => {});
  pollDiskJob(); // a cleanup may still be running from before a reload
  currentTab = safeGet("tab") || "dashboard";
  rerender();
  ensureDisclaimer().then(ensureShareNotice);
  // the heat tile waits for the first live sample; never let a skeleton spin forever
  setTimeout(() => document.querySelectorAll(".skel").forEach((el) => el.classList.remove("skel")), 8000);
})();

// ---------- GPU & AI (polled only while the page is open and the monitor is on) ----------
const pct = (v) => (v >= 0.1 ? `${v.toFixed(v < 10 ? 1 : 0)}%` : "—");
async function openGpu() {
  const tick = async () => {
    if (document.visibilityState !== "visible" || currentTab !== "gpu") return;
    const [g, ai] = await Promise.all([
      prefs.mon_gpu ? call("get_gpu").catch(() => null) : null,
      prefs.mon_ai ? call("get_ai_workload").catch(() => null) : null,
    ]);
    renderGpu(g, ai);
  };
  const gen = tabGen;
  await tick();
  if (gen !== tabGen) return; // the page changed (or was reopened) during the first read: showTab owns procTimer now
  procTimer = setInterval(tick, Math.max(2000, liveMs()));
}
function renderGpu(g, ai) {
  $("#gpu-adapters").classList.toggle("hidden", !prefs.mon_gpu);
  $("#gpu-procs-card").classList.toggle("hidden", !prefs.mon_gpu);
  $("#ai-card").classList.toggle("hidden", !prefs.mon_ai);
  if (g) {
    $("#gpu-adapters").innerHTML = g.adapters.length ? g.adapters.map((a) => `<div class="card">
      <h2><span>${a.npu ? "🧠" : "🎮"} ${esc(a.name || a.luid)}</span><span class="big-num">${pct(a.util)}</span></h2>
      ${kv([["gpu.dedicated", nz(a.dedicated) ? fmtBytes(a.dedicated) : null], ["gpu.shared", nz(a.shared) ? fmtBytes(a.shared) : null]])}
      <div class="pro-only">${(a.engines || []).filter((e) => e.util >= 0.1).map((e) => `<div class="eng"><span>${esc(e.kind)}</span><div class="meter"><i style="width:${Math.min(100, e.util)}%"></i></div><span class="num">${pct(e.util)}</span></div>`).join("")}</div>
    </div>`).join("") : `<div class="card sub">${esc(t("gpu.none"))}</div>`;
    const procs = [...g.procs].filter((p) => p.util >= 0.1 || p.dedicated > 64 * 1048576 || p.npu_util >= 0.1).sort((a, b) => b.util - a.util || b.dedicated - a.dedicated).slice(0, 40);
    $("#gpu-body").innerHTML = procs.length ? procs.map((p) => `<tr>
      <td><b>${esc(p.name || "?")}</b></td><td class="num">${p.pid}</td>
      <td class="num ${p.util > 50 ? "vhot" : p.util > 15 ? "hot" : ""}">${pct(p.util)}${p.npu_util >= 0.1 ? ` <span class="sub">NPU ${pct(p.npu_util)}</span>` : ""}</td>
      <td class="pro-only">${esc(p.engine || "")}</td><td class="num">${nz(p.dedicated) ? fmtBytes(p.dedicated) : "—"}</td>
      <td class="num pro-only">${nz(p.shared) ? fmtBytes(p.shared) : "—"}</td></tr>`).join("")
      : `<tr><td colspan="6" class="loading">${esc(t("gpu.idle"))}</td></tr>`;
  }
  if (ai) {
    $("#ai-list").innerHTML = ai.length ? ai.map((r) => `<div class="ai-row">
      <div><b>🤖 ${esc(r.label)}</b> <span class="sub">PID ${r.pid}</span>
        ${r.models?.length ? `<div class="sub">${r.models.map((m) => `${esc(m.name)}${nz(m.vram) ? ` · ${fmtBytes(m.vram)} VRAM` : ""}`).join("<br>")}</div>` : ""}</div>
      <div class="num">${nz(r.vram) ? `${fmtBytes(r.vram)} VRAM` : ""}<div class="sub">${r.util >= 0.1 ? `GPU ${pct(r.util)}` : ""}${r.npu_util >= 0.1 ? ` · NPU ${pct(r.npu_util)}` : ""}</div></div>
    </div>`).join("") : `<p class="sub">${esc(t("ai.none"))}</p>`;
  }
}

// ---------- Network ----------
const fmtRate = (b) => ltr(`${fmtBytes(b)}/s`);
async function openNetwork() {
  let n = 0;
  const tick = async () => {
    if (document.visibilityState !== "visible" || currentTab !== "network" || !prefs.mon_net) return;
    const tot = await call("get_net_totals").catch(() => null);
    if (tot) renderIfaces(tot);
    // connections change slower and cost more: every third tick
    if (n++ % 3 === 0) { const c = await call("get_connections").catch(() => null); if (c) renderConns(c); }
  };
  const gen = tabGen;
  await tick();
  if (gen !== tabGen) return;
  procTimer = setInterval(tick, Math.max(2000, liveMs()));
}
function renderIfaces(tot) {
  $("#net-ifaces").innerHTML = tot.ifaces.length ? tot.ifaces.map((i) => `<div class="card">
    <h2><span>${i.kind === "wifi" ? "📡" : "🌐"} ${esc(i.name)}</span></h2>
    ${kv([["net.down", fmtRate(i.in_per_sec)], ["net.up", fmtRate(i.out_per_sec)],
      ["net.speed", nz(i.link_speed) ? ltr(i.link_speed >= 1e9 ? `${(i.link_speed / 1e9).toFixed(1)} Gb/s` : `${Math.round(i.link_speed / 1e6)} Mb/s`) : null],
      ["net.total", ltr(`↓ ${fmtBytes(i.in_total)} · ↑ ${fmtBytes(i.out_total)}`)], ["net.adapter", esc(i.description)]])}
  </div>`).join("") : `<div class="card sub">${esc(t("net.none"))}</div>`;
}
function renderConns(c) {
  const rows = [...c.procs].sort((a, b) => b.established - a.established || b.listening.length - a.listening.length).slice(0, 60);
  $("#net-body").innerHTML = rows.length ? rows.map((p) => `<tr>
    <td><b>${esc(p.name || "?")}</b></td><td class="num">${p.pid}</td>
    <td>${[...p.listening.map((x) => `<code>${x}</code>`), ...p.udp.slice(0, 3).map((x) => `<code>udp ${x}</code>`)].slice(0, 6).join(" ") || "–"}</td>
    <td class="num">${p.established || "–"}</td>
    <td class="sub"><bdi dir="ltr">${p.remotes.map((r) => `${esc(r.host)}${r.count > 1 ? ` ×${r.count}` : ""}`).join(", ")}</bdi></td></tr>`).join("")
    : `<tr><td colspan="5" class="loading">${esc(t("net.none"))}</td></tr>`;
}
// "What holds port N" / "Who has this file open": read-only, nothing is ended from here
$("#btn-who-port").onclick = async (e) => {
  const port = Number($("#who-port").value);
  if (!(port > 0 && port < 65536)) return;
  busy(e.target.closest("button"), true);
  try {
    const r = await call("get_port_owner", { port });
    $("#who-out").innerHTML = r.length ? `<ul class="scroll-list">${r.map((o) => `<li><b>${esc(o.name)}</b> <span class="sub">PID ${o.pid} · ${esc(o.proto)} ${esc(o.states.join(", "))}</span><div class="sub mono-cut" title="${esc(o.cmd)}"><bdi dir="ltr">${esc(o.cmd)}</bdi></div></li>`).join("")}</ul>`
      : `<p class="sub">${esc(t("net.who.free", { p: port }))}</p>`;
  } catch {} finally { busy(e.target.closest("button"), false); }
};
$("#btn-who-path").onclick = async (e) => {
  const path = $("#who-path").value.trim();
  if (!path) return;
  busy(e.target.closest("button"), true);
  try {
    const r = await call("who_has_open", { path });
    $("#who-out").innerHTML = r.length ? `<ul class="scroll-list">${r.map((l) => `<li><b>${esc(l.name)}</b> <span class="sub">PID ${l.pid} · ${esc(t("net.who.kind." + l.kind))}${l.service_name ? ` · ${esc(l.service_name)}` : ""}</span></li>`).join("")}</ul>`
      : `<p class="sub">${esc(t("net.who.nobody"))}</p>`;
  } catch {} finally { busy(e.target.closest("button"), false); }
};

// ---------- Find: programs without an uninstaller, old files, files worth a look ----------
// Review lists only: nothing is removed without a click, files go to the Recycle Bin, and a
// registry entry is backed up first. "Worth a look" never says "threat": Defender gives verdicts.
const fd = { progs: null, old: null, look: null };
const fdSel = new Set();
const fmtDate = (s) => (s ? new Date(s * 1000).toLocaleDateString(LANG) : "—");
function openFinders() { for (const k of Object.keys(fd)) renderFinder(k); }
async function scanFinder(k, btn) {
  busy(btn, true);
  $(`#fd-${k}-out`).innerHTML = `<p class="scanning"><i class="scan-dot"></i>${esc(t("ready.scanning"))}</p>`;
  try {
    fd[k] = k === "progs" ? await call("finders_orphan_programs")
      : k === "old" ? await call("finders_old_files", { folders: null, minDays: null, minBytes: null })
      : await call("finders_worth_a_look");
  } catch { fd[k] = null; }
  finally { busy(btn, false); }
  [...fdSel].filter((x) => x.startsWith(k + "|")).forEach((x) => fdSel.delete(x));
  renderFinder(k);
}
const fdChk = (key, label) => `<input type="checkbox" data-fd="${esc(key)}" ${fdSel.has(key) ? "checked" : ""} aria-label="${esc(label)}" />`;
function renderFinder(k) {
  const out = $(`#fd-${k}-out`), d = fd[k];
  if (!d) { out.innerHTML = ""; return; }
  if (k === "progs") {
    if (!d.length) { out.innerHTML = `<p class="sub">✅ ${esc(t("fd.progs.none"))}</p>`; return; }
    out.innerHTML = `<table class="tbl"><thead><tr><th></th><th>${esc(t("fd.col.name"))}</th><th>${esc(t("fd.col.what"))}</th><th class="num">${esc(t("fd.col.size"))}</th></tr></thead><tbody>${d.map((p) => {
      const key = p.kind === "dead_entry" ? `progs|e|${p.reg_key}` : `progs|f|${p.path}`;
      const what = p.kind === "dead_entry"
        ? `${esc(t("fd.progs.dead"))}<div class="sub"><bdi dir="ltr">${esc(p.missing_file)}</bdi></div>`
        : `${esc(t(p.has_exe ? "fd.progs.portable" : "fd.progs.leftover"))}<div class="sub"><bdi dir="ltr">${esc(shortPath(p.path))}</bdi> · ${esc(t("fd.changed", { d: fmtDate(p.modified) }))}</div>`;
      return `<tr><td>${fdChk(key, p.name)}</td><td><b>${esc(p.name)}</b>${p.publisher ? `<div class="sub">${esc(p.publisher)}</div>` : ""}</td><td>${what}</td>
        <td class="num">${p.kind === "dead_entry" ? "—" : fmtBytes(p.bytes)}</td></tr>`;
    }).join("")}</tbody></table>
    <div class="fd-actions"><button class="danger" data-fd-act="progs" data-icon="trash-2">${esc(t("fd.progs.act"))}</button></div>`;
  } else if (k === "old") {
    if (!d.files.length) { out.innerHTML = `<p class="sub">✅ ${esc(t("fd.old.none"))}</p>`; return; }
    out.innerHTML = `<p class="summary">${esc(t("fd.old.summary", { n: d.total_found, size: fmtBytes(d.total_bytes) }))}</p>
    <table class="tbl"><thead><tr><th></th><th>${esc(t("fd.col.name"))}</th><th>${esc(t("fd.col.changed"))}</th><th class="num">${esc(t("fd.col.size"))}</th></tr></thead><tbody>${d.files.map((f) => {
      const key = `old|${f.path}`;
      return `<tr><td>${fdChk(key, f.path)}</td><td><bdi dir="ltr">${esc(shortPath(f.path))}</bdi>
        ${f.category === "old_installer" ? ` <span class="badge idle">${esc(t("fd.old.installer"))}</span>` : f.category === "old_archive" ? ` <span class="badge idle">${esc(t("fd.old.archive"))}</span>` : ""}${f.onedrive ? ` <span class="badge orphan" title="${esc(t("fd.old.onedriveTip"))}">OneDrive</span>` : ""}</td>
        <td>${esc(fmtDate(f.modified))}</td><td class="num">${fmtBytes(f.bytes)}</td></tr>`;
    }).join("")}</tbody></table>
    <div class="fd-actions"><button class="danger" data-fd-act="old" data-icon="trash-2">${esc(t("fd.old.act"))}</button></div>`;
  } else {
    if (!d.length) { out.innerHTML = `<p class="sub">✅ ${esc(t("fd.look.none"))}</p>`; return; }
    out.innerHTML = `<table class="tbl"><thead><tr><th>${esc(t("fd.col.name"))}</th><th>${esc(t("fd.col.why"))}</th><th></th></tr></thead><tbody>${d.map((i) => `<tr>
      <td><bdi dir="ltr">${esc(shortPath(i.path))}</bdi>${i.running ? ` <span class="badge serving">${esc(t("fd.look.running"))}</span>` : ""}
        <div class="sub">${esc(fmtBytes(i.bytes))} · ${esc(fmtDate(i.modified))}${i.sha256 ? `<span class="pro-only" title="SHA-256"> · ${esc(i.sha256.slice(0, 16))}…</span>` : ""}</div></td>
      <td>${esc(t("fd.look." + i.reason))}${i.autostart ? `<div class="sub"><bdi dir="ltr">${esc(i.autostart)}</bdi></div>` : ""}${i.signer ? `<div class="sub">${esc(t("fd.look.signer", { s: i.signer }))}</div>` : ""}</td>
      <td class="row-acts"><button class="small ghost" data-fd-scan-file="${esc(i.path)}" data-icon="shield-check">${esc(t("fd.look.defender"))}</button><div class="sub" data-fd-verdict="${esc(i.path)}"></div></td></tr>`).join("")}</tbody></table>`;
  }
}
// ---------- Find > What's taking space: biggest folders, one level at a time ----------
let fsNow = null;
function fillSpaceDrives() {
  const sel = $("#fd-space-drive");
  if (!sel || !volumes) return;
  const cur = sel.value;
  sel.innerHTML = volumes.map((v) => `<option value="${esc(v.drive)}\\">${ltr(esc(v.drive))} · ${esc(t("space.free", { size: fmtBytes(v.free) }))}</option>`).join("");
  if (cur) sel.value = cur;
}
async function openSizes(path, fresh = false) {
  const out = $("#fd-space-out"), btn = $("#fd-space-scan");
  busy(btn, true, t("fd.space.scanning"));
  if (!fsNow) out.innerHTML = `<p class="sub"><span class="spinner"></span> ${esc(t("fd.space.scanning"))}</p>`;
  try { fsNow = await call("get_folder_sizes", { path, fresh }); } finally { busy(btn, false); }
  renderSizes();
}
function renderSizes() {
  const r = fsNow, out = $("#fd-space-out");
  if (!r) return;
  // breadcrumb: C:\ > Users > dev
  const parts = r.path.replace(/\\+$/, "").split("\\");
  const crumbs = parts.map((p, i) => {
    const path = parts.slice(0, i + 1).join("\\") + (i === 0 ? "\\" : "");
    return i === parts.length - 1 ? `<b dir="ltr">${esc(p || path)}</b>` : `<button class="link" data-fs-go="${esc(path)}" dir="ltr">${esc(p)}</button>`;
  }).join(` <span class="sub">›</span> `);
  const max = Math.max(1, ...r.items.map((i) => i.bytes), r.files);
  const row = (name, bytes, path) => `<tr>
      <td>${path ? `<button class="link" data-fs-go="${esc(path)}">📁 <bdi dir="ltr">${esc(name)}</bdi></button>` : `<span class="sub">${esc(name)}</span>`}</td>
      <td class="fs-bar"><i style="width:${(bytes / max) * 100}%"></i></td>
      <td class="num">${fmtBytes(bytes)}</td><td class="num sub">${r.total ? Math.round((bytes / r.total) * 100) : 0}%</td>
      <td>${path ? `<button class="small ghost" data-fs-show="${esc(path)}" data-icon="folder-open">${esc(t("fd.space.show"))}</button>` : ""}</td></tr>`;
  out.innerHTML = `<p class="fs-crumbs">${crumbs} <span class="sub">· ${esc(fmtBytes(r.total))}</span></p>
    <table class="tbl"><tbody>${r.items.map((i) => row(i.name, i.bytes, i.path)).join("")}
    ${r.files ? row(t("fd.space.files"), r.files, "") : ""}</tbody></table>
    ${r.more ? `<p class="sub">${esc(t("fd.space.more", { n: r.more }))}</p>` : ""}`;
}
$("#fd-space-scan").onclick = async () => {
  if (!volumes) await loadVolumes();
  fillSpaceDrives();
  const d = $("#fd-space-drive").value;
  if (d) openSizes(fsNow && fsNow.path.toLowerCase().startsWith(d.toLowerCase()) ? fsNow.path : d, true);
};
$("#fd-space-drive").onchange = (e) => { fsNow = null; openSizes(e.target.value); };
$("#fd-space-out").addEventListener("click", (e) => {
  const go = e.target.closest("[data-fs-go]"), show = e.target.closest("[data-fs-show]");
  if (go) openSizes(go.dataset.fsGo);
  else if (show) call("show_in_explorer", { path: show.dataset.fsShow });
});
loadVolumes().then(fillSpaceDrives);

// ---------- Find > Move caches off the system drive (Dev Drive helper) ----------
let mv = null;
async function loadMoves() {
  const btn = $("#fd-move-scan");
  busy(btn, true);
  try { mv = await call("get_move_info"); } finally { busy(btn, false); }
  renderMoves();
}
function renderMoves() {
  const out = $("#fd-move-out");
  if (!mv) return;
  if (!mv.caches.length) { out.innerHTML = `<p class="sub">${esc(t("fd.move.none", { d: mv.system }))}</p>`; return; }
  if (!mv.targets.length) { out.innerHTML = `<p class="sub">${esc(t("fd.move.noDrive", { d: mv.system }))}</p>`; return; }
  const best = mv.targets.find((x) => x.dev_drive) || [...mv.targets].sort((a, b) => b.free - a.free)[0];
  const opts = mv.targets.map((x) => `<option value="${esc(x.drive)}" ${x === best ? "selected" : ""}>${ltr(esc(x.drive))} · ${esc(t("space.free", { size: fmtBytes(x.free) }))}${x.dev_drive ? " · Dev Drive" : ""}</option>`).join("");
  out.innerHTML = `<p class="fd-space-pick"><span>${esc(t("fd.move.to"))}</span> <select id="fd-move-drive">${opts}</select></p>
    <table class="tbl"><tbody>${mv.caches.map((c) => `<tr class="${c.busy ? "locked" : ""}" title="${esc(c.from)}">
      <td>${esc(t("disk.cache." + c.id))}<div class="sub"><code>${esc(c.env)}</code></div></td>
      <td class="num">${fmtBytes(c.bytes)}</td>
      <td>${c.busy ? `<span class="badge idle">${esc(t("disk.cacheBusy"))}</span>` : `<button class="small ghost" data-move="${esc(c.id)}">${esc(t("fd.move.act"))}</button>`}</td></tr>`).join("")}</tbody></table>`;
}
$("#fd-move-scan").onclick = loadMoves;
$("#fd-move-out").addEventListener("click", async (e) => {
  const b = e.target.closest("[data-move]");
  if (!b) return;
  const c = mv.caches.find((x) => x.id === b.dataset.move), drive = $("#fd-move-drive").value;
  const name = t("disk.cache." + c.id);
  const body = `<p>${esc(t("fd.move.body", { env: c.env }))}</p><p class="sub" dir="ltr">${esc(c.from)} → ${esc(drive)}\\packages\\…</p><p class="warn-text">${esc(t("fd.move.restart"))}</p>`;
  if (!(await modal(t("fd.move.confirm", { n: name, size: fmtBytes(c.bytes), d: drive }), body, { okText: t("fd.move.act") }))) return;
  busy(b, true, t("fd.move.moving"));
  try {
    const to = await call("move_cache", { id: c.id, drive });
    await loadVolumes();
    toast(t("fd.move.done", { n: name, to }) + spaceNow(), "ok", 10000);
  } finally { busy(b, false); }
  loadMoves();
});

$("#tab-finders").addEventListener("click", async (e) => {
  const scan = e.target.closest("[data-fd-scan]");
  if (scan) return scanFinder(scan.dataset.fdScan, scan);
  const df = e.target.closest("[data-fd-scan-file]");
  if (df) {
    const path = df.dataset.fdScanFile, out = [...document.querySelectorAll("[data-fd-verdict]")].find((x) => x.dataset.fdVerdict === path);
    busy(df, true);
    try {
      const r = await call("finders_defender_scan", { path });
      out.textContent = t("fd.look.v." + r.verdict);
      out.className = r.verdict === "threat" ? "warn-text" : "sub";
      if (r.verdict === "error") out.title = r.detail;
    } catch {} finally { busy(df, false); }
    return;
  }
  const act = e.target.closest("[data-fd-act]");
  if (!act) return;
  const k = act.dataset.fdAct, picked = [...fdSel].filter((x) => x.startsWith(k + "|"));
  if (!picked.length) return toast(t("fd.pickFirst"));
  if (k === "old") {
    const paths = picked.map((x) => x.slice(4));
    const size = fd.old.files.filter((f) => paths.includes(f.path)).reduce((a, f) => a + f.bytes, 0);
    const od = fd.old.files.some((f) => f.onedrive && paths.includes(f.path));
    const body = `<p>${esc(t("fd.recycleNote"))}</p>${od ? `<p class="warn-text">${esc(t("fd.old.onedriveTip"))}</p>` : ""}`;
    if (!(await modal(t("fd.old.confirm", { n: paths.length, size: fmtBytes(size) }), body, { okText: t("fd.old.act"), danger: true }))) return;
    busy(act, true);
    try {
      const r = await call("finders_recycle_files", { paths });
      const ok = r.filter((x) => x.ok).map((x) => x.path), bad = r.filter((x) => !x.ok);
      fd.old.files = fd.old.files.filter((f) => !ok.includes(f.path));
      ok.forEach((p) => fdSel.delete(`old|${p}`));
      toast(t("fd.done", { n: ok.length }) + (bad.length ? "\n" + bad.map((b) => `${b.path}: ${b.error}`).join("\n") : ""), bad.length ? "error" : "ok");
    } catch {} finally { busy(act, false); }
    return renderFinder("old");
  }
  // programs: dead uninstall entries (registry, backed up first) and leftover folders (Recycle Bin)
  const items = picked.map((x) => ({ kind: x[6], id: x.slice(8) }));
  if (!(await modal(t("fd.progs.confirm", { n: items.length }), `<p>${esc(t("fd.progs.confirmBody"))}</p>`, { okText: t("fd.progs.act"), danger: true }))) return;
  busy(act, true);
  const fails = [];
  for (const it of items) {
    try {
      if (it.kind === "e") await call("finders_remove_dead_entry", { regKey: it.id });
      else await call("finders_recycle_leftover", { path: it.id });
      fd.progs = fd.progs.filter((p) => (p.kind === "dead_entry" ? p.reg_key : p.path) !== it.id);
      fdSel.delete(`progs|${it.kind}|${it.id}`);
    } catch (err) { fails.push(String(err)); }
  }
  busy(act, false);
  toast(t("fd.done", { n: items.length - fails.length }) + (fails.length ? "\n" + fails.join("\n") : ""), fails.length ? "error" : "ok");
  renderFinder("progs");
});
$("#tab-finders").addEventListener("change", (e) => {
  const k = e.target.dataset.fd;
  if (!k) return;
  if (e.target.checked) fdSel.add(k); else fdSel.delete(k);
});
