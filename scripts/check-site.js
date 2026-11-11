// Checks the homepage's machine-readable files and links before they go live:
// sitemap URLs, internal links and rigsweep.com links in llms.txt / index.md all point to real files,
// JSON-LD parses and has the expected types, every page has canonical + Open Graph tags,
// llms.txt follows the llmstxt.org layout, robots.txt points to the sitemap.
// Usage: node scripts/check-site.js   (CI runs this; exits 1 on any problem)
const fs = require("fs"), path = require("path");
const docs = path.join(__dirname, "..", "docs");
const read = (f) => fs.readFileSync(path.join(docs, f), "utf8");
let problems = 0;
const fail = (where, msg) => { problems++; console.log(`✗ ${where}: ${msg}`); };

// "/", "/cli/", "/llms.txt", "/screenshots/x.png" -> file in docs/
const fileFor = (urlPath) => {
  const p = decodeURI(urlPath.split(/[?#]/)[0]);
  return path.join(docs, p.endsWith("/") ? p + "index.html" : p);
};
const exists = (urlPath) => fs.existsSync(fileFor(urlPath));

const pages = ["index.html", "404.html", ...fs.readdirSync(docs).filter((d) => fs.existsSync(path.join(docs, d, "index.html"))).map((d) => `${d}/index.html`)];

// ---- every page: internal links, canonical, Open Graph ----
for (const f of pages) {
  const html = read(f);
  for (const [, href] of html.matchAll(/(?:href|src)="([^"]+)"/g)) {
    if (href.startsWith("/") && !href.startsWith("//") && !exists(href)) fail(f, `broken link ${href}`);
    const abs = href.match(/^https:\/\/rigsweep\.com(\/.*)$/);
    if (abs && !exists(abs[1])) fail(f, `broken link ${href}`);
  }
  if (f === "404.html") continue;
  if (!/<link rel="canonical" href="https:\/\/rigsweep\.com\/[^"]*"/.test(html)) fail(f, "missing canonical");
  if (!/<html lang="\w+/.test(html)) fail(f, "missing <html lang>");
  for (const prop of ["og:type", "og:title", "og:description", "og:image", "og:url"]) {
    if (!html.includes(`property="${prop}"`)) fail(f, `missing ${prop}`);
  }
  if ((html.match(/<h1[\s>]/g) || []).length !== 1) fail(f, "needs exactly one <h1>");
}

// ---- JSON-LD ----
const home = read("index.html");
const ld = home.match(/<script type="application\/ld\+json">([\s\S]*?)<\/script>/);
if (!ld) fail("index.html", "no JSON-LD");
else {
  try {
    const types = JSON.parse(ld[1])["@graph"].map((n) => n["@type"]);
    for (const t of ["Organization", "WebSite", "SoftwareApplication", "FAQPage"]) if (!types.includes(t)) fail("index.html", `JSON-LD lacks ${t}`);
  } catch (e) { fail("index.html", `JSON-LD doesn't parse: ${e.message}`); }
}

// ---- sitemap ----
const sitemap = read("sitemap.xml");
if (!sitemap.startsWith('<?xml version="1.0" encoding="UTF-8"?>')) fail("sitemap.xml", "missing XML declaration");
const locs = [...sitemap.matchAll(/<loc>https:\/\/rigsweep\.com(\/[^<]*)<\/loc>/g)].map((m) => m[1]);
if (!locs.length) fail("sitemap.xml", "no URLs");
for (const l of locs) if (!exists(l)) fail("sitemap.xml", `${l} has no file`);
for (const f of pages) {
  if (f === "404.html") continue;
  const url = "/" + f.replace(/index\.html$/, "");
  if (!locs.includes(url)) fail("sitemap.xml", `page ${url} is not listed`);
}

// ---- robots.txt ----
if (!/^Sitemap: https:\/\/rigsweep\.com\/sitemap\.xml$/m.test(read("robots.txt"))) fail("robots.txt", "doesn't point to the sitemap");

// ---- llms.txt (https://llmstxt.org): H1, blockquote summary, H2 sections of links ----
const llms = read("llms.txt");
if (!/^# \S/.test(llms)) fail("llms.txt", "must start with an H1");
if (!/^> \S/m.test(llms)) fail("llms.txt", "needs a > summary");
if (!/^## When to use/m.test(llms)) fail("llms.txt", "needs a 'When to use' section");
for (const f of ["llms.txt", "index.md"]) {
  for (const [, p] of read(f).matchAll(/https:\/\/rigsweep\.com(\/[^\s)]*)/g)) if (!exists(p)) fail(f, `broken link ${p}`);
}

if (problems) { console.log(`\n${problems} problem(s)`); process.exit(1); }
console.log(`site OK: ${pages.length} pages, ${locs.length} sitemap URLs, llms.txt, JSON-LD`);
