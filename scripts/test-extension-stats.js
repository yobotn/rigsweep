// node scripts/test-extension-stats.js — checks stats.js with a stubbed chrome API and fetch.
const fs = require('fs');
const path = require('path');
const vm = require('vm');
const assert = require('assert');

function sandbox({ language = 'en-US', timeZone = 'America/New_York' } = {}) {
  const local = {}, sync = { autoSleep: true };
  const sent = [];
  const area = store => ({
    get: async k => (typeof k === 'string' ? (k in store ? { [k]: structuredClone(store[k]) } : {}) : { ...k, ...structuredClone(store) }),
    set: async o => { Object.assign(store, structuredClone(o)); },
  });
  const ctx = {
    console, structuredClone, crypto: require('crypto').webcrypto, Uint8Array, JSON, Date, Math, Promise, parseInt,
    Intl: { DateTimeFormat: () => ({ resolvedOptions: () => ({ timeZone }) }) },
    navigator: { language, userAgentData: { brands: [{ brand: 'Not A Brand', version: '99' }, { brand: 'Google Chrome', version: '131' }] }, userAgent: '' },
    chrome: {
      storage: { local: area(local), sync: area(sync) },
      tabs: { query: async () => Array.from({ length: 42 }, (_, i) => ({ id: i, url: `https://secret-site-${i}.example/` })) },
      windows: { getAll: async () => [{}, {}] },
      runtime: { getManifest: () => ({ version: '1.1.0' }) },
    },
    fetch: async (url, opts = {}) => { sent.push({ url, method: opts.method || 'GET', body: opts.body }); return { ok: true }; },
  };
  vm.createContext(ctx);
  vm.runInContext(fs.readFileSync(path.join(__dirname, '..', 'chrome-extension', 'stats.js'), 'utf8') + '\nthis.Stats = Stats;', ctx);
  return { Stats: ctx.Stats, sent, local };
}

(async () => {
  // region decides the default
  assert.equal(sandbox({ language: 'en-US', timeZone: 'America/New_York' }).Stats.consentDefault(), 'opt_out');
  assert.equal(sandbox({ language: 'he-IL', timeZone: 'Asia/Jerusalem' }).Stats.consentDefault(), 'opt_out');
  assert.equal(sandbox({ language: 'de-DE', timeZone: 'Europe/Berlin' }).Stats.consentDefault(), 'opt_in');
  assert.equal(sandbox({ language: 'en-GB', timeZone: 'America/New_York' }).Stats.consentDefault(), 'opt_in');
  assert.equal(sandbox({ language: 'en-US', timeZone: 'Europe/Paris' }).Stats.consentDefault(), 'opt_in');
  assert.equal(sandbox({ language: 'en', timeZone: '' }).Stats.consentDefault(), 'opt_in');

  // opt-out region: on, but nothing is sent before the notice is answered
  let { Stats, sent } = sandbox();
  let s = await Stats.state();
  assert.equal(s.enabled, true);
  assert.equal(s.noticeSeen, false);
  assert.match(s.installId, /^[0-9a-f]{32}$/);
  await Stats.bump('slept', 5);
  await Stats.bump('dupes', 2);
  assert.equal(await Stats.sendIfDue(), false);
  assert.equal(sent.length, 0);

  await Stats.noticeSeen();
  assert.equal(await Stats.sendIfDue(), true);
  assert.equal(sent.length, 1);
  const body = JSON.parse(sent[0].body);
  assert.deepEqual(Object.keys(body).sort(), ['auto_sleep', 'browser', 'browser_major', 'dupes_closed', 'ext_version', 'install_id',
    'matches_closed', 'tabs_open', 'tabs_slept', 'v', 'windows_open']);
  assert.equal(body.tabs_slept, 5);
  assert.equal(body.tabs_open, 42);
  assert.equal(body.browser, 'Chrome');
  assert.equal(body.browser_major, 131);
  assert.ok(!sent[0].body.includes('secret-site'), 'no URLs leave the browser');

  // weekly: not again right away; pending counts were reset, lifetime totals kept
  assert.equal(await Stats.sendIfDue(), false);
  s = await Stats.state();
  assert.equal(s.total.slept, 5);
  assert.equal((await Stats.build()).tabs_slept, 0);

  // turning off: DELETE for the old id, a new id, nothing more sent
  const oldId = s.installId;
  s = await Stats.setShare(false);
  assert.equal(s.enabled, false);
  assert.notEqual(s.installId, oldId);
  assert.equal(sent.at(-1).method, 'DELETE');
  assert.ok(sent.at(-1).url.endsWith(`/v1/install/${oldId}`));

  // opt-in region: off until yes
  ({ Stats, sent } = sandbox({ language: 'fr-FR', timeZone: 'Europe/Paris' }));
  s = await Stats.state();
  assert.equal(s.enabled, false);
  await Stats.noticeSeen();
  assert.equal(await Stats.sendIfDue(), false);
  await Stats.setShare(true);
  assert.equal(await Stats.sendIfDue(), true);
  console.log('stats.js: all checks passed');
})().catch(e => { console.error(e); process.exit(1); });
