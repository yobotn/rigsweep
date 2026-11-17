// RigSweep Tabs - anonymous hourly stats: counts only, never URLs, titles, sites or patterns.
// Loaded by the popup and (via importScripts) by the service worker.
//
// On by default outside Europe, but nothing is sent before the popup's first-run notice has been
// answered. Turning sharing off asks the server to delete what this install sent and starts a new id.
/* exported Stats */
const Stats = (() => {
  const SERVER = 'https://stats.rigsweep.com';
  const HOUR = 3600 * 1000;
  const KEY = 'stats';

  // EU/EEA, UK, Switzerland: sharing waits for a yes there (same list as the Windows app).
  const ASK_FIRST = ['AT', 'BE', 'BG', 'HR', 'CY', 'CZ', 'DK', 'EE', 'FI', 'FR', 'DE', 'GR', 'HU', 'IE', 'IT', 'LV', 'LT', 'LU', 'MT',
    'NL', 'PL', 'PT', 'RO', 'SK', 'SI', 'ES', 'SE', 'IS', 'LI', 'NO', 'GB', 'CH'];

  // Decided in the browser from its language region and time zone; never sent. Unknown or
  // conflicting signals take the careful side.
  function consentDefault() {
    let tz = '';
    try { tz = Intl.DateTimeFormat().resolvedOptions().timeZone || ''; } catch { /* unknown */ }
    const m = /-([A-Za-z]{2})(?:-|$)/.exec(navigator.language || '');
    const region = m ? m[1].toUpperCase() : '';
    const tzEurope = /^(Europe\/|Atlantic\/(Reykjavik|Canary|Madeira|Faroe|Faeroe|Azores))/.test(tz);
    if (region && ASK_FIRST.includes(region)) return 'opt_in';
    if (tzEurope) return 'opt_in';
    if (region || tz) return 'opt_out';
    return 'opt_in';
  }

  function newId() {
    const b = crypto.getRandomValues(new Uint8Array(16));
    return [...b].map(x => x.toString(16).padStart(2, '0')).join('');
  }

  async function load() {
    const { [KEY]: s = {} } = await chrome.storage.local.get(KEY);
    s.pending ??= { slept: 0, dupes: 0, matches: 0 };
    s.total ??= { slept: 0, dupes: 0, matches: 0, since: Date.now() };
    if (!/^[0-9a-f]{32}$/.test(s.installId || '')) s.installId = newId();
    return s;
  }
  const save = s => chrome.storage.local.set({ [KEY]: s });

  function enabledOf(s) {
    if (s.share === 'on') return true;
    if (s.share === 'off') return false;
    return consentDefault() === 'opt_out';
  }

  async function state() {
    const s = await load();
    await save(s);
    return {
      enabled: enabledOf(s), chosen: !!s.share, noticeSeen: !!s.notice, consentDefault: consentDefault(),
      installId: s.installId, lastSent: s.last || 0, total: s.total,
    };
  }

  // Count what the extension did: 'slept' | 'dupes' | 'matches'.
  async function bump(what, n) {
    if (!n) return;
    const s = await load();
    s.pending[what] = (s.pending[what] || 0) + n;
    s.total[what] = (s.total[what] || 0) + n;
    await save(s);
  }

  function browser() {
    const brands = navigator.userAgentData?.brands || [];
    const find = re => brands.find(b => re.test(b.brand));
    const pick = [[/Edge/, 'Edge'], [/Opera/, 'Opera'], [/Brave/, 'Brave'], [/Vivaldi/, 'Vivaldi'], [/Google Chrome/, 'Chrome'], [/Chromium/, 'Chromium']];
    for (const [re, name] of pick) {
      const b = find(re);
      if (b) return { browser: navigator.brave && name !== 'Brave' ? 'Brave' : name, browser_major: parseInt(b.version, 10) || 0 };
    }
    const v = /Chrome\/(\d+)/.exec(navigator.userAgent || '');
    return { browser: 'Other', browser_major: v ? parseInt(v[1], 10) : 0 };
  }

  // UTC to the minute, the format the server stores: "2026-11-17T14:05Z"
  const minute = t => new Date(t).toISOString().slice(0, 16) + 'Z';

  // Exactly what the next report holds (also the preview): counts since the last accepted report.
  async function build() {
    const s = await load();
    const [tabs, wins, settings] = await Promise.all([chrome.tabs.query({}), chrome.windows.getAll(), chrome.storage.sync.get({ autoSleep: false })]);
    return {
      v: 1, install_id: s.installId, ext_version: chrome.runtime.getManifest().version,
      ...(s.last ? { since: minute(s.last) } : {}), ...browser(),
      tabs_slept: s.pending.slept, dupes_closed: s.pending.dupes, matches_closed: s.pending.matches,
      tabs_open: tabs.length, windows_open: wins.length, auto_sleep: !!settings.autoSleep,
    };
  }

  async function forget(id) {
    try { await fetch(`${SERVER}/v1/install/${id}`, { method: 'DELETE' }); } catch { /* best effort */ }
  }

  // Send if allowed and an hour has passed. Returns true when a report was accepted.
  async function sendIfDue() {
    let s = await load();
    if (!s.notice || !enabledOf(s) || Date.now() - (s.last || 0) < HOUR) return false;
    const r = await build();
    let ok = false;
    try {
      const res = await fetch(`${SERVER}/v1/ext/report`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(r) });
      ok = res.ok;
    } catch { /* offline or server down: try again later */ }
    if (!ok) return false;
    s = await load();
    if (s.installId !== r.install_id || !enabledOf(s)) { await forget(r.install_id); return false; }
    // only what was sent leaves "pending"; anything counted while sending stays for next time
    s.pending.slept -= r.tabs_slept; s.pending.dupes -= r.dupes_closed; s.pending.matches -= r.matches_closed;
    s.last = Date.now();
    s.lastReport = r;
    await save(s);
    return true;
  }

  function rotate(s) {
    s.installId = newId();
    s.last = 0;
    s.lastReport = null;
    s.pending = { slept: 0, dupes: 0, matches: 0 };
  }

  // An explicit choice (also counts as having seen the notice).
  async function setShare(on) {
    const s = await load();
    const wasOn = enabledOf(s);
    s.share = on ? 'on' : 'off';
    s.notice = true;
    let old = null;
    if (!on && wasOn) { old = s.installId; rotate(s); }
    await save(s);
    if (old) await forget(old);
    return state();
  }

  // The notice was shown and the default was kept.
  async function noticeSeen() {
    const s = await load();
    s.notice = true;
    await save(s);
    return state();
  }

  async function resetId() {
    const s = await load();
    const old = s.installId;
    rotate(s);
    await save(s);
    await forget(old);
    return state();
  }

  return { consentDefault, state, bump, build, sendIfDue, setShare, noticeSeen, resetId };
})();
