// One command for everything CI checks (G7): JS syntax, translations, the site, the extension stats
// test, and the Rust tests of the app (src-tauri) and the stats server (server/).
// Usage: npm run check            everything (the server tests download Postgres the first time; the
//                                accessibility check needs `npx playwright install chromium` once)
//        npm run check -- --fast  skip the Rust tests and the accessibility check (seconds instead of minutes)
const { spawnSync } = require("child_process");
const fs = require("fs"), path = require("path");
const repo = path.join(__dirname, "..");
const fast = process.argv.includes("--fast");
const results = [];

function step(name, cmd, args, cwd = repo) {
  process.stdout.write(`\n▶ ${name}\n`);
  const t = Date.now();
  const r = spawnSync(cmd, args, { cwd, stdio: "inherit" });
  const ok = r.status === 0;
  results.push([name, ok, ((Date.now() - t) / 1000).toFixed(1)]);
  return ok;
}

// JS files must at least parse (the app has no bundler that would catch it)
step("JS syntax", process.execPath, ["-e", `
  const fs = require("fs");
  const files = ["ui/app.js", "ui/i18n.js", "ui/icons.js", "chrome-extension/popup.js", "chrome-extension/background.js",
    ...fs.readdirSync("ui/locales").map((f) => "ui/locales/" + f), ...fs.readdirSync("docs/locales").map((f) => "docs/locales/" + f)];
  for (const f of files) new Function(fs.readFileSync(f, "utf8"));
  console.log("ok: " + files.length + " files parse");`]);
step("Translations in sync", process.execPath, ["scripts/check-i18n.js"]);
step("Site prerender up to date", process.execPath, ["scripts/prerender-site.js", "--check"]);
step("Site links, JSON-LD, OpenAPI", process.execPath, ["scripts/check-site.js"]);
step("Extension stats", process.execPath, ["scripts/test-extension-stats.js"]);
if (!fast) step("Site accessibility (axe)", process.execPath, ["scripts/check-a11y.js", "--all"]);
if (!fast) {
  step("App tests (src-tauri)", "cargo", ["test", "--locked"], path.join(repo, "src-tauri"));
  if (fs.existsSync(path.join(repo, "server", "Cargo.toml"))) step("Server tests (server)", "cargo", ["test", "--locked"], path.join(repo, "server"));
}

console.log("\n" + results.map(([n, ok, s]) => `${ok ? "✓" : "✗"} ${n} (${s}s)`).join("\n"));
if (fast) console.log("(--fast: Rust tests skipped)");
const failed = results.filter((r) => !r[1]).length;
console.log(failed ? `\n${failed} step(s) failed` : "\nall checks passed");
process.exit(failed ? 1 : 0);
