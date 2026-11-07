// RigSweep Tabs - popup UI. All tab data goes into the DOM via textContent only.
const DEFAULTS = { idleMinutes: 30, autoSleep: false, patterns: ['localhost:*', '127.0.0.1:*'] };
const $ = id => document.getElementById(id);
const t = (key, ...subs) => chrome.i18n.getMessage(key, subs.map(String));

function status(text, isError = false) {
  $('status').textContent = text;
  $('status').classList.toggle('err', isError);
}

// Fill every [data-i18n] element with its localized string.
document.querySelectorAll('[data-i18n]').forEach(el => { el.textContent = t(el.dataset.i18n); });
document.title = t('extName');

async function refreshStats() {
  const [tabs, wins] = await Promise.all([chrome.tabs.query({}), chrome.windows.getAll()]);
  $('statTabs').textContent = tabs.length;
  $('statSleeping').textContent = tabs.filter(x => x.discarded).length;
  $('statWindows').textContent = wins.length;
}

// --- Settings ---
async function loadSettings() {
  const s = await chrome.storage.sync.get(DEFAULTS);
  $('idleMinutes').value = s.idleMinutes;
  $('autoSleep').checked = s.autoSleep;
  $('patterns').value = s.patterns.join('\n');
}
const idleMinutes = () => Math.max(1, Math.min(1440, parseInt($('idleMinutes').value, 10) || DEFAULTS.idleMinutes));
$('idleMinutes').addEventListener('change', () => {
  $('idleMinutes').value = idleMinutes();
  chrome.storage.sync.set({ idleMinutes: idleMinutes() });
});
$('autoSleep').addEventListener('change', () => chrome.storage.sync.set({ autoSleep: $('autoSleep').checked }));

// --- Sleep ---
async function sleep(ignoreAge) {
  const r = await chrome.runtime.sendMessage({ cmd: 'sleep', minutes: idleMinutes(), ignoreAge });
  if (!r || r.error) return status(t('errorGeneric'), true);
  status(t('sleptResult', r.slept) + (r.failed ? ' ' + t('sleptFailed', r.failed) : ''));
  refreshStats();
}
$('btnSleepIdle').addEventListener('click', () => sleep(false));
$('btnSleepAll').addEventListener('click', () => sleep(true));

// --- Duplicates: same URL ignoring #hash; keep active, then pinned, then most recently accessed ---
let dupeIds = [];
const stripHash = url => (url || '').split('#')[0];
$('btnFindDupes').addEventListener('click', async () => {
  const groups = new Map();
  for (const tab of await chrome.tabs.query({})) {
    const key = stripHash(tab.url || tab.pendingUrl);
    if (key) groups.set(key, [...(groups.get(key) || []), tab]);
  }
  const rank = x => (x.active ? 2e15 : 0) + (x.pinned ? 1e15 : 0) + (x.lastAccessed || 0);
  dupeIds = [];
  for (const tabs of groups.values()) {
    if (tabs.length < 2) continue;
    tabs.sort((a, b) => rank(b) - rank(a));
    dupeIds.push(...tabs.slice(1).filter(x => !x.pinned).map(x => x.id));
  }
  $('btnCloseDupes').hidden = !dupeIds.length;
  $('btnCloseDupes').textContent = t('btnCloseDupes', dupeIds.length);
  status(dupeIds.length ? t('dupesFound', dupeIds.length) : t('noDupes'));
});
$('btnCloseDupes').addEventListener('click', async () => {
  await closeTabs(dupeIds, 'dupesClosed');
  dupeIds = [];
  $('btnCloseDupes').hidden = true;
});

async function closeTabs(ids, msgKey) {
  try {
    await chrome.tabs.remove(ids);
    status(t(msgKey, ids.length));
  } catch { status(t('errorGeneric'), true); }
  refreshStats();
}

// --- Patterns: '*' wildcard matched against host+path. A pattern without '/' also matches any path. ---
function globToRegExp(glob) {
  const body = glob.replace(/[.+?^${}()|[\]\\/]/g, '\\$&').replace(/\*/g, '.*');
  return new RegExp('^' + body + (glob.includes('/') ? '' : '(?:/.*)?') + '$', 'i');
}
const readPatterns = () => $('patterns').value.split('\n').map(s => s.trim()).filter(Boolean);
$('btnSavePatterns').addEventListener('click', async () => {
  await chrome.storage.sync.set({ patterns: readPatterns() });
  status(t('patternsSaved'));
});

$('btnFindMatches').addEventListener('click', async () => {
  const regs = readPatterns().map(globToRegExp);
  const includePinned = $('includePinned').checked;
  const list = $('matches');
  list.replaceChildren();
  const matches = (await chrome.tabs.query({})).filter(tab => {
    if (tab.pinned && !includePinned) return false;
    try {
      const u = new URL(tab.url || tab.pendingUrl);
      return regs.some(r => r.test(u.host + u.pathname));
    } catch { return false; }
  });
  for (const tab of matches) {
    const li = document.createElement('li');
    const box = Object.assign(document.createElement('input'), { type: 'checkbox', checked: true });
    box.dataset.id = tab.id;
    const text = document.createElement('div');
    const title = Object.assign(document.createElement('div'), { className: 'title', textContent: tab.title || tab.url });
    const host = Object.assign(document.createElement('div'), { className: 'host', textContent: new URL(tab.url || tab.pendingUrl).host });
    text.append(title, host);
    li.append(box, text);
    list.append(li);
  }
  list.hidden = $('btnCloseSelected').hidden = !matches.length;
  status(matches.length ? t('matchesFound', matches.length) : t('noMatches'));
});
$('btnCloseSelected').addEventListener('click', async () => {
  const ids = [...$('matches').querySelectorAll('input:checked')].map(b => Number(b.dataset.id));
  if (!ids.length) return;
  await closeTabs(ids, 'matchesClosed');
  $('matches').replaceChildren();
  $('matches').hidden = $('btnCloseSelected').hidden = true;
});

loadSettings();
refreshStats();
