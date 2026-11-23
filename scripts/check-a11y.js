// Accessibility check for the website (WCAG 2.2 AA, engineering standard TST-08 / UX): serves docs/ locally,
// opens each page in headless Chromium and runs axe-core on it.
// Usage: node scripts/check-a11y.js        (needs `npx playwright install chromium` once)
//        node scripts/check-a11y.js --all  every page, not just one per kind
const http = require("http"), fs = require("fs"), path = require("path");
const { chromium } = require("playwright");
const { AxeBuilder } = require("@axe-core/playwright");

const docs = path.join(__dirname, "..", "docs");
const TYPES = { ".html": "text/html; charset=utf-8", ".js": "text/javascript", ".css": "text/css", ".png": "image/png",
  ".svg": "image/svg+xml", ".woff2": "font/woff2", ".json": "application/json", ".txt": "text/plain" };

// one page of each kind by default: home, a language home (right-to-left too), guide, compare, docs pages
const PAGES = ["/", "/de/", "/ar/", "/guides/", "/guides/port-already-in-use-windows/", "/compare/",
  "/compare/ccleaner/", "/cli/", "/developers/", "/about/", "/privacy/", "/contact/"];
const all = () => {
  const out = [];
  const walk = (rel) => {
    for (const e of fs.readdirSync(path.join(docs, rel), { withFileTypes: true })) {
      if (!e.isDirectory()) continue;
      const r = `${rel}/${e.name}`;
      if (fs.existsSync(path.join(docs, r, "index.html"))) out.push(r + "/");
      walk(r);
    }
  };
  walk("");
  return ["/", ...out.sort()];
};

const server = http.createServer((req, res) => {
  let p = decodeURIComponent(req.url.split("?")[0]);
  if (p.endsWith("/")) p += "index.html";
  const f = path.join(docs, p);
  if (!f.startsWith(docs) || !fs.existsSync(f) || fs.statSync(f).isDirectory()) { res.writeHead(404); return res.end(); }
  res.writeHead(200, { "content-type": TYPES[path.extname(f)] || "application/octet-stream" });
  fs.createReadStream(f).pipe(res);
});

(async () => {
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const base = `http://127.0.0.1:${server.address().port}`;
  const browser = await chromium.launch();
  // the live counters call stats.rigsweep.com; block outside requests so the check is offline and repeatable
  const context = await browser.newContext();
  await context.route((url) => !url.href.startsWith(base), (route) => route.abort());
  let problems = 0;
  for (const p of process.argv.includes("--all") ? all() : PAGES) {
    const page = await context.newPage();
    await page.goto(base + p, { waitUntil: "load" });
    const r = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"]).analyze();
    for (const v of r.violations) {
      problems++;
      console.log(`✗ ${p}: ${v.id} (${v.impact}) ${v.help}`);
      for (const n of v.nodes.slice(0, 3)) console.log(`    ${n.target.join(" ")}  ${(n.failureSummary || "").split("\n")[1] || ""}`.trimEnd());
    }
    await page.close();
  }
  await browser.close();
  server.close();
  console.log(problems ? `\n${problems} accessibility problem(s)` : "accessibility OK (axe, WCAG 2.2 AA)");
  process.exit(problems ? 1 : 0);
})().catch((e) => { console.error(e); process.exit(2); });
