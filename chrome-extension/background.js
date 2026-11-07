// RigSweep Tabs - service worker: idle-tab sleeping + auto-sleep alarm.
const DEFAULTS = { idleMinutes: 30, autoSleep: false, patterns: ['localhost:*', '127.0.0.1:*'] };
const ALARM = 'rigsweep-autosleep';

const getSettings = () => chrome.storage.sync.get(DEFAULTS);

// Discard background tabs. Sleeping keeps the tab in the strip; it reloads when clicked.
async function sleepTabs({ minutes = DEFAULTS.idleMinutes, ignoreAge = false } = {}) {
  const cutoff = Date.now() - minutes * 60000;
  const tabs = await chrome.tabs.query({});
  const targets = tabs.filter(t =>
    !t.active && !t.pinned && !t.audible && !t.discarded && t.autoDiscardable !== false &&
    (ignoreAge || (t.lastAccessed ?? Date.now()) < cutoff));
  let slept = 0, failed = 0;
  await Promise.all(targets.map(async t => {
    try { (await chrome.tabs.discard(t.id)) ? slept++ : failed++; } // some tabs can't be discarded
    catch { failed++; }
  }));
  return { slept, failed };
}

// Create or clear the 10-minute alarm to match the autoSleep setting.
async function syncAlarm() {
  const { autoSleep } = await getSettings();
  const existing = await chrome.alarms.get(ALARM);
  if (autoSleep && !existing) await chrome.alarms.create(ALARM, { periodInMinutes: 10 });
  if (!autoSleep && existing) await chrome.alarms.clear(ALARM);
}

chrome.runtime.onInstalled.addListener(syncAlarm);
chrome.runtime.onStartup.addListener(syncAlarm);
chrome.storage.onChanged.addListener((changes, area) => {
  if (area === 'sync' && changes.autoSleep) syncAlarm();
});

chrome.alarms.onAlarm.addListener(async alarm => {
  if (alarm.name !== ALARM) return;
  const { idleMinutes, autoSleep } = await getSettings();
  if (autoSleep) await sleepTabs({ minutes: idleMinutes });
});

chrome.runtime.onMessage.addListener((msg, _sender, reply) => {
  if (msg?.cmd !== 'sleep') return false;
  sleepTabs(msg).then(reply, () => reply({ slept: 0, failed: 0, error: true }));
  return true; // async response
});
