// Bakes the English homepage text into docs/index.html so search engines and AI agents that don't
// run JavaScript see the full page (the language switcher still swaps it at runtime). Also keeps the
// JSON-LD (version + FAQ) in sync with docs/locales/en.js and writes docs/index.md, the Markdown
// version of the homepage for agents, plus docs/<page>/index.md for every inner page (nested ones too).
// Usage: node scripts/prerender-site.js          rewrite the files
//        node scripts/prerender-site.js --check  exit 1 if they are out of date (CI)
const fs = require("fs"), path = require("path");
const repo = path.join(__dirname, "..");
const p = (f) => path.join(repo, f);
const read = (f) => fs.readFileSync(p(f), "utf8");

const T = {};
new Function("window", "T", read("docs/locales/en.js"))({ T }, T);
const en = T.en;
const version = JSON.parse(read("src-tauri/tauri.conf.json")).version;

const esc = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
const strip = (s) => s.replace(/<[^>]+>/g, "").replace(/&amp;/g, "&");
// Markdown: <b>/<strong> -> **, <code> -> `, <a href> -> [text](href), other tags dropped
const md = (s) => s
  .replace(/<(b|strong)>(.*?)<\/\1>/g, "**$2**")
  .replace(/<code>(.*?)<\/code>/g, "`$1`")
  .replace(/<a href="([^"]+)">(.*?)<\/a>/g, "[$2]($1)")
  .replace(/<[^>]+>/g, "");

// ---- index.html, plus one homepage per language at docs/<lang>/index.html ----
// The language pages are what search engines index for each language (hreflang links them all);
// the picker on the page moves between them. Hebrew is in the app only, never on the site.
const LANG_NAMES = { en: "English", de: "Deutsch", es: "Español", fr: "Français", it: "Italiano", "pt-BR": "Português (Brasil)",
  ru: "Русский", ja: "日本語", ko: "한국어", "zh-CN": "简体中文", ar: "العربية" };
const LANGS = Object.keys(LANG_NAMES);
const OG_LOCALE = { en: "en_US", de: "de_DE", es: "es_ES", fr: "fr_FR", it: "it_IT", "pt-BR": "pt_BR", ru: "ru_RU", ja: "ja_JP", ko: "ko_KR", "zh-CN": "zh_CN", ar: "ar_AR" };
const RTL = ["ar"];
for (const l of LANGS) if (l !== "en") new Function("window", "T", read(`docs/locales/${l}.js`))({ T }, T);
const urlFor = (l) => `https://rigsweep.com/${l === "en" ? "" : l + "/"}`;
const attr = (s) => esc(s).replace(/"/g, "&quot;");

const missing = [];
const bake = (h, dict) => h.replace(/<(\w+)([^>]*?)\sdata-t(-html)?="([^"]+)"([^>]*)>([\s\S]*?)<\/\1>/g, (m, tag, a, isHtml, key, b) => {
  if (!(key in en)) { missing.push(key); return m; }
  const v = dict[key] ?? en[key];
  return `<${tag}${a} data-t${isHtml || ""}="${key}"${b}>${isHtml ? v : esc(v)}</${tag}>`;
});
const faqKeys = Object.keys(en).filter((k) => /^faq\.q\d+$/.test(k));
const faqLd = (dict) => faqKeys.map((q) => "          " + JSON.stringify({
  "@type": "Question", name: strip(dict[q] ?? en[q]),
  acceptedAnswer: { "@type": "Answer", text: strip(dict[q.replace(".q", ".a")] ?? en[q.replace(".q", ".a")]) },
}));
// cache buster: ?v=<content hash> on scripts and screenshots, so returning visitors get new files without a hard reload
const crypto = require("crypto");
const hash = (rel) => crypto.createHash("sha1").update(fs.readFileSync(p("docs" + rel)).toString("latin1").replace(/\r\n/g, "\n"), "latin1").digest("hex").slice(0, 8);
// brand files (favicon, icon, mark, social card) too, on every page: browsers and Cloudflare cache them for hours
const BRAND_FILES = /((?:src|href|content)=")((?:https:\/\/rigsweep\.com)?\/(favicon-32\.png|icon\.png|mark\.svg|social\.png))(?:\?v=\w+)?"/g;
const bustBrand = (h) => h.replace(BRAND_FILES, (m, a, url, file) => `${a}${url}?v=${hash("/" + file)}"`);
const bust = (h) => bustBrand(h.replace(/src="(\/(?:locales\/[\w-]+\.js|icons\.js|screenshots\/[\w-]+\.png))(?:\?v=\w+)?"/g, (m, rel) => `src="${rel}?v=${hash(rel)}"`));
const hreflang = ["<!--hreflang-->", ...LANGS.map((l) => `  <link rel="alternate" hreflang="${l}" href="${urlFor(l)}" />`),
  `  <link rel="alternate" hreflang="x-default" href="${urlFor("en")}" />`, "  <!--/hreflang-->"].join("\n");
const langNav = (cur) => LANGS.map((l) => `<a href="${l === "en" ? "/" : "/" + l + "/"}" hreflang="${l}" lang="${l}"${l === cur ? ' aria-current="page"' : ""}>${LANG_NAMES[l]}</a>`).join(" ");

function homepage(src, l) {
  const dict = T[l];
  let h = bake(src, dict);
  h = h.replace(/<html[^>]*>/, l === "en" ? '<html lang="en">' : `<html lang="${l}"${RTL.includes(l) ? ' dir="rtl"' : ""} data-page-lang="${l}">`);
  h = h.replace(/<title>[^<]*<\/title>/, `<title>${esc(dict["meta.title"])}</title>`);
  h = h.replace(/(<meta name="description" content=")[^"]*"/, `$1${attr(dict["meta.desc"])}"`);
  h = h.replace(/(<meta (?:property="og|name="twitter):description" content=")[^"]*"/g, `$1${attr(dict["meta.desc"])}"`);
  h = h.replace(/(<link rel="canonical" href=")[^"]*"/, `$1${urlFor(l)}"`);
  h = h.replace(/(<meta property="og:url" content=")[^"]*"/, `$1${urlFor(l)}"`);
  h = h.replace(/(<meta property="og:locale" content=")[^"]*"/, `$1${OG_LOCALE[l]}"`);
  h = h.includes("<!--hreflang-->") ? h.replace(/<!--hreflang-->[\s\S]*?<!--\/hreflang-->/, hreflang)
    : h.replace(/(<link rel="canonical"[^>]*>)/, `$1\n  ${hreflang}`);
  h = h.replace(/<!--langs-->[\s\S]*?<!--\/langs-->/, `<!--langs-->${langNav(l)}<!--/langs-->`);
  h = h.replace(/"softwareVersion": "[^"]*"/, `"softwareVersion": "${version}"`);
  h = h.replace(/("@type": "FAQPage",[\s\S]*?"mainEntity": \[\r?\n)[\s\S]*?(\r?\n\s*\])/, `$1${faqLd(dict).join(",\n")}$2`);
  // the hero shot in the page language, where a translated one exists
  if (l !== "en" && fs.existsSync(p(`docs/screenshots/cleanup-${l}.png`))) h = h.replace(/src="\/screenshots\/cleanup\.png(\?v=\w+)?"/, `src="/screenshots/cleanup-${l}.png"`);
  return bust(h);
}
const source = read("docs/index.html");
const html = homepage(source, "en");
const langPages = Object.fromEntries(LANGS.filter((l) => l !== "en").map((l) => [l, homepage(source, l)]));
if (missing.length) { console.error("✗ keys missing from en.js: " + [...new Set(missing)].join(", ")); process.exit(1); }
for (const l of LANGS) for (const k of ["meta.title", "meta.desc"]) if (!T[l][k]) { console.error(`✗ docs/locales/${l}.js lacks ${k}`); process.exit(1); }

// ---- index.md ----
const list = (prefix) => Object.keys(en).filter((k) => k.startsWith(prefix)).map((k) => `- ${md(en[k])}`).join("\n");
const feats = Object.keys(en).filter((k) => /^f\d+\.h$/.test(k)).map((k) => `- **${en[k]}**: ${md(en[k.replace(".h", ".p")])}`).join("\n");
const faq = faqKeys.map((q) => `### ${strip(en[q])}\n\n${md(en[q.replace(".q", ".a")])}`).join("\n\n");
const index = `# RigSweep: room for your agents

> ${strip(en["hero.sub"])}

- Download: https://github.com/yobotn/rigsweep/releases/latest (Windows 10 / 11, free, MIT)
- Source: https://github.com/yobotn/rigsweep
- Current version: ${version}
- For AI agents: https://rigsweep.com/llms.txt

## ${strip(en["story.h"]).replace(/^\S+ /, "")}

${en["story.p1"]}

${en["story.p2"]}

${en["story.p2b"]}

${en["story.p2c"]}

${md(en["story.p3"])}

## Features

${feats}

## ${strip(en["clean.h"]).replace(/^\S+ /, "")}

${list("clean.l")}

${md(en["eco.p"])}

## ${strip(en["safe.h"]).replace(/^\S+ /, "")}

${list("safe.l")}

## FAQ

${faq}

## ${strip(en["trust.h"]).replace(/^\S+ /, "")}

${md(en["trust.p1"])}

${md(en["trust.p2"])}

## Privacy

${en["sign.p"]} Full policy: https://rigsweep.com/privacy/

## Disclaimer

${en["disc.p"]}

## Links

- [Guides](https://rigsweep.com/guides/) · [Compare](https://rigsweep.com/compare/) · [API and developer docs](https://rigsweep.com/developers/) · [CLI reference](https://rigsweep.com/cli/) · [About](https://rigsweep.com/about/) · [Privacy](https://rigsweep.com/privacy/) · [Contact](https://rigsweep.com/contact/)
- [Vote on upcoming features](https://github.com/yobotn/rigsweep/issues/12)
- [Report an issue](https://github.com/yobotn/rigsweep/issues)
`;

// ---- <page>/index.md for every inner page ----
// The server sends these to clients that ask for Accept: text/markdown (see server/src/web.rs).
// The pages use a small set of tags (h1-h3, p, ul/li, pre, code, a, strong, em), all handled here.
const decode = (s) => s.replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&amp;/g, "&");
const inline = (s) => decode(s
  .replace(/<code>([\s\S]*?)<\/code>/g, "`$1`")
  .replace(/<(strong|b)>([\s\S]*?)<\/\1>/g, "**$2**")
  .replace(/<em>([\s\S]*?)<\/em>/g, "*$1*")
  .replace(/<a href="([^"]+)"[^>]*>([\s\S]*?)<\/a>/g, (m, href, text) => `[${text}](${href.startsWith("/") ? "https://rigsweep.com" + href : href})`)
  .replace(/<[^>]+>/g, "")
  .replace(/\s*\n\s*/g, " ")).trim();
const pageMd = (page) => {
  const main = page.match(/<main[^>]*>([\s\S]*?)<\/main>/)[1];
  const desc = page.match(/<meta name="description" content="([^"]+)"/);
  const blocks = [];
  // pre blocks keep their text verbatim; everything else is one block element per match
  const re = /<pre[^>]*><code>([\s\S]*?)<\/code><\/pre>|<h([1-6])[^>]*>([\s\S]*?)<\/h\2>|<p[^>]*>([\s\S]*?)<\/p>|<ul[^>]*>([\s\S]*?)<\/ul>|<table[^>]*>([\s\S]*?)<\/table>|<figure>[\s\S]*?<img src="([^"]+)" alt="([^"]*)"[\s\S]*?<\/figure>/g;
  for (const m of main.matchAll(re)) {
    if (m[1] !== undefined) blocks.push("```\n" + decode(m[1]) + "\n```");
    else if (m[7] !== undefined) blocks.push(`![${decode(m[8])}](${m[7].startsWith("/") ? "https://rigsweep.com" + m[7] : m[7]})`);
    else if (m[6] !== undefined) {
      // a table becomes a pipe table: one row per <tr>, th and td cells alike
      const rows = [...m[6].matchAll(/<tr[^>]*>([\s\S]*?)<\/tr>/g)].map((tr) =>
        [...tr[1].matchAll(/<t[hd][^>]*>([\s\S]*?)<\/t[hd]>/g)].map((c) => inline(c[1]).replace(/\|/g, "\\|") || " "));
      if (rows.length) blocks.push([rows[0], rows[0].map(() => "---"), ...rows.slice(1)].map((r) => `| ${r.join(" | ")} |`).join("\n"));
    }
    else if (m[2]) {
      blocks.push(`${"#".repeat(+m[2])} ${inline(m[3])}`);
      if (m[2] === "1" && desc) blocks.push(`> ${decode(desc[1])}`);
    } else if (m[4] !== undefined) blocks.push(inline(m[4]));
    else blocks.push([...m[5].matchAll(/<li[^>]*>([\s\S]*?)<\/li>/g)].map((li) => `- ${inline(li[1])}`).join("\n"));
  }
  return blocks.join("\n\n") + "\n";
};
const out = { "docs/index.html": html, "docs/index.md": index };
for (const [l, h] of Object.entries(langPages)) { out[`docs/${l}/index.html`] = h; out[`docs/${l}/index.md`] = pageMd(h); }
// inner pages can be nested (docs/guides/<slug>/index.html), so walk the folders
const walk = (dir) => fs.readdirSync(p(dir), { withFileTypes: true }).filter((e) => e.isDirectory()).sort((a, b) => a.name.localeCompare(b.name))
  .flatMap((e) => [`${dir}/${e.name}`, ...walk(`${dir}/${e.name}`)]);
for (const d of walk("docs")) {
  if (!(`${d}/index.html` in out) && fs.existsSync(p(`${d}/index.html`))) out[`${d}/index.html`] = bustBrand(read(`${d}/index.html`));
  if (!(`${d}/index.md` in out) && fs.existsSync(p(`${d}/index.html`))) out[`${d}/index.md`] = pageMd(read(`${d}/index.html`));
}
out["docs/404.html"] = bustBrand(read("docs/404.html"));

// ---- docs/guides/feed.xml: RSS 2.0 of the guides, newest first, full content with absolute links.
// The site is the source of truth for articles: dev.to (and similar) import new posts from this
// feed with rigsweep.com as the canonical URL.
const xml = (t) => t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
const guides = walk("docs/guides").filter((d) => fs.existsSync(p(`${d}/index.html`))).map((d) => {
  const h = read(`${d}/index.html`);
  const ld = (k) => decode(h.match(new RegExp(`"${k}": "([^"]+)"`))[1]);
  const body = h.match(/<main[^>]*>([\s\S]*?)<\/main>/)[1]
    .replace(/<div class="crumbs">[\s\S]*?<\/div>/, "").replace(/<h1>[\s\S]*?<\/h1>/, "").replace(/<p class="date">[\s\S]*?<\/p>/, "")
    .replace(/(href|src)="\/(?!\/)/g, '$1="https://rigsweep.com/').replace(/\s*\r?\n\s*/g, "\n").trim();
  return { title: ld("headline"), desc: ld("description"), url: ld("url"), date: ld("datePublished"), body };
}).sort((a, b) => b.date.localeCompare(a.date) || a.url.localeCompare(b.url));
out["docs/guides/feed.xml"] = [
  '<?xml version="1.0" encoding="UTF-8"?>',
  '<rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom" xmlns:content="http://purl.org/rss/1.0/modules/content/">',
  "<channel>",
  "<title>RigSweep guides</title>",
  "<link>https://rigsweep.com/guides/</link>",
  "<description>Fixes for Windows developer machines: leftover processes, busy ports, dev caches and disk space.</description>",
  "<language>en</language>",
  '<atom:link href="https://rigsweep.com/guides/feed.xml" rel="self" type="application/rss+xml" />',
  ...guides.map((g) => [
    "<item>", `<title>${xml(g.title)}</title>`, `<link>${g.url}</link>`, `<guid isPermaLink="true">${g.url}</guid>`,
    `<pubDate>${new Date(`${g.date}T08:00:00Z`).toUTCString()}</pubDate>`, `<description>${xml(g.desc)}</description>`,
    `<content:encoded><![CDATA[${g.body.replace(/]]>/g, "]]]]><![CDATA[>")}]]></content:encoded>`, "</item>",
  ].join("\n")),
  "</channel>", "</rss>", "",
].join("\n");

// ---- README story: the "why I built it" section of all 11 READMEs is generated from the site's
// story.* and stat.* strings (docs/locales) and the stat numbers in docs/index.html, so the site and
// GitHub always tell the same story. Edit it there, never in a README.
const STATS = [...source.matchAll(/data-t="stat\.(\w+)">[^<]*<\/span><b>([^<]+)<\/b>/g)].map((m) => [m[1], m[2]]);
if (STATS.length < 4) { console.error("✗ docs/index.html: stat panel not found"); process.exit(1); }
for (const l of LANGS) {
  const f = l === "en" ? "README.md" : `README.${l}.md`;
  const g = (k) => md(T[l][k] ?? en[k]);
  const cur = read(f), nl = cur.includes("\r\n") ? "\r\n" : "\n";
  const story = [
    "<!-- Generated from docs/locales (story.*, stat.*) by scripts/prerender-site.js. Edit it there. -->",
    "", g("story.p1"), "", g("story.p2"), "",
    ...STATS.map(([k, v]) => `- ${g("stat." + k)}: **${v}**`),
    "", `*${g("stat.cap")}*`, "", g("story.p2b"), "", g("story.p2c"), "", `> ${g("story.p3")}`, "",
  ].join(nl);
  // the block between the first "## " heading after the badges and the next "---" rule
  const re = /(\n---\r?\n\r?\n## [^\r\n]+\r?\n\r?\n)[\s\S]*?(\r?\n---\r?\n)/;
  if (!re.test(cur)) { console.error(`✗ ${f}: story section not found`); process.exit(1); }
  out[f] = cur.replace(re, (m, a, b) => a + story + b);
}
let stale = 0;
for (const [f, s] of Object.entries(out)) {
  // compare without line endings: Windows CI checks files out with CRLF
  const cur = fs.existsSync(p(f)) ? read(f) : "";
  if (cur.replace(/\r\n/g, "\n") === s.replace(/\r\n/g, "\n")) continue;
  if (process.argv.includes("--check")) { stale++; console.log(`✗ ${f} is out of date: run node scripts/prerender-site.js`); }
  else { fs.mkdirSync(path.dirname(p(f)), { recursive: true }); fs.writeFileSync(p(f), s); console.log(`✓ wrote ${f}`); }
}
process.exit(stale ? 1 : 0);
