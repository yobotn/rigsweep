// Checks that every language is in sync with English across the app, the Chrome extension, the
// homepage and the READMEs: same keys, same {placeholders} / $VARS$, same HTML tags, and READMEs
// with the same structure (headings, tables, identical commands, same links).
// Usage: node scripts/check-i18n.js            (CI runs this; exits 1 on any problem)
const fs = require("fs"), path = require("path");
const repo = path.join(__dirname, "..");
const read = (f) => fs.readFileSync(path.join(repo, f), "utf8");
const LANGS = ["de", "es", "fr", "it", "pt-BR", "ru", "ja", "ko", "zh-CN"];
let problems = 0;
const fail = (where, msg) => { problems++; console.log(`✗ ${where}: ${msg}`); };

const ph = (s) => (s.match(/\{\w+\}/g) || []).sort().join(",");
const tags = (s) => (s.match(/<\/?[a-z]+\b/g) || []).sort().join(",");
const nl = (s) => (s.match(/\n/g) || []).length;

function compareTables(where, en, xx, { placeholders = true, newlines = true } = {}) {
  if (!xx) return fail(where, "not loaded");
  for (const k of Object.keys(en)) {
    if (!(k in xx)) { fail(where, `missing ${k}`); continue; }
    if (placeholders && ph(en[k]) !== ph(xx[k])) fail(where, `placeholders differ in ${k}`);
    if (tags(en[k]) !== tags(xx[k])) fail(where, `HTML tags differ in ${k}`);
    if (newlines && nl(en[k]) !== nl(xx[k])) fail(where, `line breaks differ in ${k}`);
  }
  for (const k of Object.keys(xx)) if (!(k in en)) fail(where, `extra key ${k}`);
}

// ---- app UI ----
const appSrc = read("ui/i18n.js");
const I18N = new Function(appSrc.split("// Native names")[0] + "; return I18N;")();
for (const l of LANGS) {
  try { new Function("I18N", read(`ui/locales/${l}.js`))(I18N); } catch (e) { fail(`ui/locales/${l}.js`, e.message); }
}
for (const l of LANGS) compareTables(`ui/locales/${l}.js`, I18N.en, I18N[l]);
for (const l of LANGS) if (!appSrc.includes(`"${l}"`) && !appSrc.includes(` ${l}:`)) fail("ui/i18n.js", `LANGS lacks ${l}`);
const html = read("ui/index.html");
for (const l of LANGS) if (!html.includes(`locales/${l}.js`)) fail("ui/index.html", `doesn't load locales/${l}.js`);

// ---- Chrome extension ----
const ce = JSON.parse(read("chrome-extension/_locales/en/messages.json"));
for (const l of LANGS) {
  const f = `chrome-extension/_locales/${l.replace("-", "_")}/messages.json`;
  let cx;
  try { cx = JSON.parse(read(f)); } catch (e) { fail(f, e.message); continue; }
  const vars = (s) => (s.match(/\$\w+\$/g) || []).sort().join();
  for (const k of Object.keys(ce)) {
    if (!cx[k]?.message) fail(f, `missing ${k}`);
    else if (JSON.stringify(ce[k].placeholders || null) !== JSON.stringify(cx[k].placeholders || null)) fail(f, `placeholders block differs in ${k}`);
    else if (vars(ce[k].message) !== vars(cx[k].message)) fail(f, `$VARS$ differ in ${k}`);
  }
  for (const k of Object.keys(cx)) if (!(k in ce)) fail(f, `extra key ${k}`);
}

// ---- homepage ----
const T = {};
for (const l of ["en", ...LANGS]) {
  try { new Function("window", "T", read(`docs/locales/${l}.js`))({ T }, T); } catch (e) { fail(`docs/locales/${l}.js`, e.message); }
}
for (const l of LANGS) compareTables(`docs/locales/${l}.js`, T.en, T[l], { placeholders: false, newlines: false });
const site = read("docs/index.html");
for (const l of LANGS) if (!site.includes(`locales/${l}.js`)) fail("docs/index.html", `doesn't load locales/${l}.js`);
for (const l of LANGS) if (!fs.existsSync(path.join(repo, `docs/screenshots/cleanup-${l}.png`))) fail("docs/screenshots", `cleanup-${l}.png missing`);

// ---- READMEs ----
const E = read("README.md");
const heads = (s) => s.split("\n").filter((x) => /^#{1,3} /.test(x)).map((x) => x.match(/^#+/)[0].length).join("");
const fences = (s) => (s.match(/^```/gm) || []).length;
const blocks = (s) => [...s.matchAll(/^```[^\n]*\n([\s\S]*?)^```/gm)].map((m) => m[1].replace(/#.*$/gm, "").trim()).join("\n---\n");
const rows = (s) => (s.match(/^\|/gm) || []).length;
const links = (s) => [...s.matchAll(/\]\((https?:[^)\s]+|[^)#\s][^)\s]*)\)/g)].map((m) => m[1])
  .filter((u) => !/^README\./.test(u) && !/screenshots\//.test(u)).sort().join("\n");
for (const l of LANGS) {
  const f = `README.${l}.md`;
  let X;
  try { X = read(f); } catch { fail(f, "missing"); continue; }
  if (heads(E) !== heads(X)) fail(f, "heading structure differs from README.md");
  if (fences(E) !== fences(X) || blocks(E) !== blocks(X)) fail(f, "code blocks differ (commands must be identical)");
  if (rows(E) !== rows(X)) fail(f, "table rows differ");
  if (links(E) !== links(X)) fail(f, "links differ");
  for (const o of ["README.md", ...LANGS.filter((x) => x !== l).map((x) => `README.${x}.md`)])
    if (!X.includes(`(${o})`)) fail(f, `language bar lacks ${o}`);
}
for (const l of LANGS) if (!E.includes(`(README.${l}.md)`)) fail("README.md", `language bar lacks README.${l}.md`);

console.log(problems ? `\n${problems} problem(s)` : `i18n in sync: ${LANGS.length + 1} languages (app ${Object.keys(I18N.en).length} keys, site ${Object.keys(T.en).length} keys, READMEs)`);
process.exit(problems ? 1 : 0);
