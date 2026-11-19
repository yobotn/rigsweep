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

// inner pages, nested ones included (guides/<slug>/index.html)
const dirs = (rel) => fs.readdirSync(path.join(docs, rel), { withFileTypes: true }).filter((e) => e.isDirectory())
  .flatMap((e) => { const r = rel ? `${rel}/${e.name}` : e.name; return [r, ...dirs(r)]; });
const pages = ["index.html", "404.html", ...dirs("").filter((d) => fs.existsSync(path.join(docs, d, "index.html"))).map((d) => `${d}/index.html`)];

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
  if (f !== "index.html") {
    for (const [, json] of html.matchAll(/<script type="application\/ld\+json">([\s\S]*?)<\/script>/g)) {
      try {
        const nodes = JSON.parse(json)["@graph"] || [];
        // FAQPage questions must match the FAQ shown on the page
        for (const n of nodes) if (n["@type"] === "FAQPage") for (const q of n.mainEntity || []) {
          if (!html.includes(`<h3>${q.name.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")}</h3>`)) fail(f, `FAQ JSON-LD question not on the page: ${q.name}`);
        }
      } catch (e) { fail(f, `JSON-LD doesn't parse: ${e.message}`); }
    }
  }
}

// ---- JSON-LD ----
const home = read("index.html");
const ld = home.match(/<script type="application\/ld\+json">([\s\S]*?)<\/script>/);
if (!ld) fail("index.html", "no JSON-LD");
else {
  try {
    const types = JSON.parse(ld[1])["@graph"].map((n) => n["@type"]);
    for (const t of ["Organization", "WebSite", "SoftwareApplication", "WebAPI", "FAQPage"]) if (!types.includes(t)) fail("index.html", `JSON-LD lacks ${t}`);
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

// ---- Markdown twins: every page has <page>/index.md for Accept: text/markdown ----
for (const f of pages) {
  if (f === "404.html") continue;
  const mdf = f.replace(/index\.html$/, "index.md");
  if (!fs.existsSync(path.join(docs, mdf))) { fail(mdf, "missing (run node scripts/prerender-site.js)"); continue; }
  const md = read(mdf);
  if (!/^# \S/.test(md) || md.length < 500) fail(mdf, "needs an H1 and at least 500 characters");
  for (const [, raw] of md.matchAll(/https:\/\/rigsweep\.com(\/[^\s)`"'}]*)/g)) {
    const p = raw.replace(/[.,;:]+$/, "");
    if (!exists(p)) fail(mdf, `broken link ${p}`);
  }
}

// ---- OpenAPI 3.1: unique operationIds, descriptions, typed responses, refs that resolve ----
let spec;
try { spec = JSON.parse(read("openapi.json")); } catch (e) { fail("openapi.json", `doesn't parse: ${e.message}`); }
if (spec) {
  if (!/^3\.1\.\d+$/.test(spec.openapi || "")) fail("openapi.json", "openapi must be 3.1.x");
  for (const k of ["title", "version", "description"]) if (!spec.info?.[k]) fail("openapi.json", `info.${k} missing`);
  if (!spec.servers?.length) fail("openapi.json", "no servers");
  const resolve = (ref) => ref.replace(/^#\//, "").split("/").reduce((o, k) => o?.[k.replace(/~1/g, "/").replace(/~0/g, "~")], spec);
  (function walk(o, at) {
    if (!o || typeof o !== "object") return;
    if (typeof o.$ref === "string" && resolve(o.$ref) === undefined) fail("openapi.json", `${at}: $ref ${o.$ref} doesn't resolve`);
    for (const [k, v] of Object.entries(o)) walk(v, `${at}/${k}`);
  })(spec, "#");
  const ids = new Set();
  for (const [route, item] of Object.entries(spec.paths || {})) {
    for (const [method, op] of Object.entries(item)) {
      const where = `${method.toUpperCase()} ${route}`;
      if (!op.operationId || !/^[A-Za-z][A-Za-z0-9_]*$/.test(op.operationId)) fail("openapi.json", `${where}: missing or invalid operationId`);
      else if (ids.has(op.operationId)) fail("openapi.json", `${where}: duplicate operationId ${op.operationId}`);
      ids.add(op.operationId);
      if (!op.summary || !op.description) fail("openapi.json", `${where}: needs summary and description`);
      for (const prm of op.parameters || []) if (!prm.schema || !prm.description) fail("openapi.json", `${where}: parameter ${prm.name} needs a schema and description`);
      for (const [code, r] of Object.entries(op.responses || {})) {
        const res = r.$ref ? resolve(r.$ref) : r;
        if (!res?.description) fail("openapi.json", `${where} ${code}: no description`);
        if (+code >= 400 && !res?.content?.["application/problem+json"]) fail("openapi.json", `${where} ${code}: errors must be application/problem+json`);
      }
      if (!op.responses) fail("openapi.json", `${where}: no responses`);
    }
  }
  // every error code the API can send is documented on the developer page (the problem `type` links there)
  const dev = read("developers/index.html");
  for (const code of spec.components?.schemas?.Problem?.properties?.code?.enum || []) {
    if (!dev.includes(`id="error-${code}"`)) fail("developers/index.html", `no #error-${code} entry`);
  }
}

// ---- RFC 9727 API catalog (RFC 9264 linkset) ----
try {
  const cat = JSON.parse(read(".well-known/api-catalog"));
  if (!Array.isArray(cat.linkset) || !cat.linkset.length) fail("api-catalog", "needs a linkset array");
  for (const ctx of cat.linkset || []) {
    if (!ctx.anchor) fail("api-catalog", "every link context needs an anchor");
    for (const rel of ["service-desc", "service-doc"]) {
      if (!ctx[rel]?.length) fail("api-catalog", `missing ${rel}`);
      for (const l of ctx[rel] || []) { const m = l.href?.match(/^https:\/\/rigsweep\.com(\/.*)$/); if (m && !exists(m[1])) fail("api-catalog", `${rel} ${l.href} has no file`); }
    }
  }
} catch (e) { fail("api-catalog", `doesn't parse: ${e.message}`); }

// ---- the homepage links the docs, the spec and the catalog ----
for (const href of ["/developers/", "https://rigsweep.com/openapi.json", "https://rigsweep.com/.well-known/api-catalog"]) {
  if (!home.includes(`href="${href}"`)) fail("index.html", `doesn't link ${href}`);
}

if (problems) { console.log(`\n${problems} problem(s)`); process.exit(1); }
console.log(`site OK: ${pages.length} pages + Markdown twins, ${locs.length} sitemap URLs, llms.txt, JSON-LD, OpenAPI, API catalog`);
