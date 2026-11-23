// Tells IndexNow (Bing, Yandex, Seznam, Naver and the AI search engines built on them) which pages
// changed in a deploy, so they recrawl them now instead of in a few weeks. The key is the
// docs/<32 hex>.txt file whose content is its own name.
// Usage: node scripts/indexnow.js <previous commit>   (deploy.yml; without a commit it sends every sitemap URL)
//        add --dry-run to print the URLs without sending them
const fs = require("fs"), path = require("path"), { execFileSync } = require("child_process");
const repo = path.join(__dirname, "..");
const docs = path.join(repo, "docs");
const HOST = "rigsweep.com";

const keyFile = fs.readdirSync(docs).find((f) => /^[0-9a-f]{32}\.txt$/.test(f) && fs.readFileSync(path.join(docs, f), "utf8").trim() === f.slice(0, -4));
if (!keyFile) { console.error("✗ no IndexNow key file in docs/"); process.exit(1); }
const key = keyFile.slice(0, -4);

const sitemap = fs.readFileSync(path.join(docs, "sitemap.xml"), "utf8");
const all = [...sitemap.matchAll(/<loc>(https:\/\/rigsweep\.com\/[^<]*)<\/loc>/g)].map((m) => m[1]);
const homepages = all.filter((u) => /^https:\/\/rigsweep\.com\/([a-z]{2}(-[A-Z]{2})?\/)?$/.test(u));

const before = process.argv.slice(2).find((a) => !a.startsWith("--"));
let urls;
let changed = null;
if (before && !/^0+$/.test(before)) {
  try { changed = execFileSync("git", ["diff", "--name-only", before, "HEAD", "--", "docs"], { cwd: repo, encoding: "utf8" }).split("\n").filter(Boolean); }
  catch { console.log("previous commit not available, sending every sitemap URL"); }
}
if (!changed) urls = all;
else {
  const set = new Set();
  for (const f of changed) {
    const rel = f.replace(/^docs\//, "");
    // the homepage is built from index.html, the locales and the screenshots, in every language
    if (/^(index\.html|index\.md|locales\/|screenshots\/|icons\.js|social\.png)/.test(rel)) homepages.forEach((u) => set.add(u));
    else if (/(^|\/)index\.(html|md)$/.test(rel)) set.add(`https://${HOST}/${rel.replace(/index\.(html|md)$/, "")}`);
    else if (/^(llms\.txt|sitemap\.xml)$/.test(rel)) set.add(`https://${HOST}/${rel}`);
  }
  urls = [...set].filter((u) => all.includes(u) || u.endsWith("/llms.txt") || u.endsWith("/sitemap.xml"));
}
if (!urls.length) { console.log("IndexNow: no public page changed"); process.exit(0); }
console.log(`IndexNow: ${urls.length} URL(s)\n` + urls.join("\n"));
if (process.argv.includes("--dry-run")) process.exit(0);

fetch("https://api.indexnow.org/indexnow", {
  method: "POST",
  headers: { "content-type": "application/json; charset=utf-8" },
  body: JSON.stringify({ host: HOST, key, keyLocation: `https://${HOST}/${keyFile}`, urlList: urls }),
}).then(async (r) => {
  // 200 = received, 202 = accepted, key check pending
  console.log(`IndexNow answered ${r.status} ${r.statusText}`);
  if (r.status >= 400) { console.log(await r.text()); process.exit(1); }
}).catch((e) => { console.error("IndexNow request failed:", e.message); process.exit(1); });
