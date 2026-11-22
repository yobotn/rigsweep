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

// ---- index.html ----
let html = read("docs/index.html");
const missing = [];
html = html.replace(/<(\w+)([^>]*?)\sdata-t(-html)?="([^"]+)"([^>]*)>([\s\S]*?)<\/\1>/g, (m, tag, a, isHtml, key, b) => {
  if (!(key in en)) { missing.push(key); return m; }
  return `<${tag}${a} data-t${isHtml || ""}="${key}"${b}>${isHtml ? en[key] : esc(en[key])}</${tag}>`;
});
if (missing.length) { console.error("✗ keys missing from en.js: " + missing.join(", ")); process.exit(1); }
html = html.replace(/"softwareVersion": "[^"]*"/, `"softwareVersion": "${version}"`);
const faqKeys = Object.keys(en).filter((k) => /^faq\.q\d+$/.test(k));
const faqLd = faqKeys.map((q) => "          " + JSON.stringify({
  "@type": "Question", name: strip(en[q]), acceptedAnswer: { "@type": "Answer", text: strip(en[q.replace(".q", ".a")]) },
}));
html = html.replace(/("@type": "FAQPage",[\s\S]*?"mainEntity": \[\n)[\s\S]*?(\n\s*\])/, `$1${faqLd.join(",\n")}$2`);

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
  const re = /<pre><code>([\s\S]*?)<\/code><\/pre>|<h([1-6])[^>]*>([\s\S]*?)<\/h\2>|<p[^>]*>([\s\S]*?)<\/p>|<ul[^>]*>([\s\S]*?)<\/ul>|<table[^>]*>([\s\S]*?)<\/table>/g;
  for (const m of main.matchAll(re)) {
    if (m[1] !== undefined) blocks.push("```\n" + decode(m[1]) + "\n```");
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
// inner pages can be nested (docs/guides/<slug>/index.html), so walk the folders
const walk = (dir) => fs.readdirSync(p(dir), { withFileTypes: true }).filter((e) => e.isDirectory()).sort((a, b) => a.name.localeCompare(b.name))
  .flatMap((e) => [`${dir}/${e.name}`, ...walk(`${dir}/${e.name}`)]);
for (const d of walk("docs")) {
  if (fs.existsSync(p(`${d}/index.html`))) out[`${d}/index.md`] = pageMd(read(`${d}/index.html`));
}
let stale = 0;
for (const [f, s] of Object.entries(out)) {
  // compare without line endings: Windows CI checks files out with CRLF
  const cur = fs.existsSync(p(f)) ? read(f) : "";
  if (cur.replace(/\r\n/g, "\n") === s.replace(/\r\n/g, "\n")) continue;
  if (process.argv.includes("--check")) { stale++; console.log(`✗ ${f} is out of date: run node scripts/prerender-site.js`); }
  else { fs.writeFileSync(p(f), s); console.log(`✓ wrote ${f}`); }
}
process.exit(stale ? 1 : 0);
