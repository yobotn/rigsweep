// RigSweep brand studio: a local-only page that renders the app icon, installer icons, favicon,
// Chrome extension icons and the Open Graph card from one mark, and writes them into the repo.
//
// Usage: npm run brand   (then open http://127.0.0.1:4747)
//
// Local only: binds to 127.0.0.1, serves a short list of read-only folders, and writes only to the
// fixed targets below. Nothing here is deployed (the site is docs/).
const http = require("http");
const fs = require("fs");
const path = require("path");
const { spawn } = require("child_process");

const ROOT = path.resolve(__dirname, "..");
const PORT = Number(process.env.BRAND_PORT) || 4747;

// folders the page may read
const READ = ["brand", "docs/screenshots", "docs/fonts", "ui/fonts"];

// every file the page may write: target key -> [repo path, pixel size]
const TARGETS = {
  "app-master": ["brand/app-icon.png", 1024],
  "site-favicon": ["docs/favicon-32.png", 32],
  "site-icon": ["docs/icon.png", 128],
  "ext-16": ["chrome-extension/icons/icon16.png", 16],
  "ext-32": ["chrome-extension/icons/icon32.png", 32],
  "ext-48": ["chrome-extension/icons/icon48.png", 48],
  "ext-128": ["chrome-extension/icons/icon128.png", 128],
  "og": ["docs/social.png", null],
  "brand-config": ["brand/brand.json", null],
};

const TYPES = { ".html": "text/html; charset=utf-8", ".js": "text/javascript", ".css": "text/css", ".json": "application/json",
  ".svg": "image/svg+xml", ".png": "image/png", ".woff2": "font/woff2", ".txt": "text/plain; charset=utf-8" };

function send(res, code, body, type = "application/json") {
  res.writeHead(code, { "Content-Type": type, "Cache-Control": "no-store" });
  res.end(typeof body === "string" || Buffer.isBuffer(body) ? body : JSON.stringify(body));
}

function readBody(req, limit = 32 * 1024 * 1024) {
  return new Promise((ok, fail) => {
    const chunks = []; let n = 0;
    req.on("data", (c) => { n += c.length; if (n > limit) { fail(new Error("too large")); req.destroy(); } else chunks.push(c); });
    req.on("end", () => ok(Buffer.concat(chunks).toString("utf8")));
    req.on("error", fail);
  });
}

function pngFromDataUrl(s) {
  const m = /^data:image\/png;base64,([A-Za-z0-9+/=]+)$/.exec(s || "");
  if (!m) throw new Error("expected a PNG data URL");
  return Buffer.from(m[1], "base64");
}

function run(cmd, args) {
  return new Promise((ok) => {
    const p = spawn(cmd, args, { cwd: ROOT, shell: process.platform === "win32" });
    let out = "";
    p.stdout.on("data", (d) => (out += d)); p.stderr.on("data", (d) => (out += d));
    p.on("close", (code) => ok({ code, out }));
  });
}

// `tauri icon` also emits Android/iOS/Appx sets we don't ship; keep only what tauri.conf.json uses
// plus the Square*/StoreLogo files that were already in the repo.
async function tauriIcons() {
  const dir = path.join(ROOT, "src-tauri/icons");
  const before = new Set(fs.readdirSync(dir));
  const r = await run("npx", ["tauri", "icon", "brand/app-icon.png", "-o", "src-tauri/icons"]);
  for (const f of fs.readdirSync(dir)) {
    if (!before.has(f)) fs.rmSync(path.join(dir, f), { recursive: true, force: true });
  }
  return r;
}

// ICO with PNG frames (Windows Vista and later): 6-byte header, a 16-byte entry per frame, then the PNGs
function icoFile(frames) {
  const head = Buffer.alloc(6 + 16 * frames.length);
  head.writeUInt16LE(0, 0); head.writeUInt16LE(1, 2); head.writeUInt16LE(frames.length, 4);
  let offset = head.length;
  frames.forEach(([n, png], i) => {
    const e = 6 + 16 * i;
    head.writeUInt8(n >= 256 ? 0 : n, e); head.writeUInt8(n >= 256 ? 0 : n, e + 1);
    head.writeUInt16LE(1, e + 4); head.writeUInt16LE(32, e + 6);
    head.writeUInt32LE(png.length, e + 8); head.writeUInt32LE(offset, e + 12);
    offset += png.length;
  });
  return Buffer.concat([head, ...frames.map(([, png]) => png)]);
}

const server = http.createServer(async (req, res) => {
  try {
    const url = new URL(req.url, `http://${req.headers.host}`);
    // only answer requests addressed to this machine (blocks DNS-rebinding pages)
    if (!/^(127\.0\.0\.1|localhost)(:\d+)?$/.test(req.headers.host || "")) return send(res, 403, { error: "local only" });

    if (req.method === "GET" && url.pathname === "/") { res.writeHead(302, { Location: "/brand/" }); return res.end(); }

    if (req.method === "GET" && url.pathname === "/api/screenshots") {
      const files = fs.readdirSync(path.join(ROOT, "docs/screenshots")).filter((f) => f.endsWith(".png")).sort();
      return send(res, 200, files);
    }

    if (req.method === "GET" && url.pathname === "/api/targets") {
      return send(res, 200, Object.fromEntries(Object.entries(TARGETS).map(([k, [p, s]]) => [k, { path: p, size: s }])));
    }

    if (req.method === "POST" && url.pathname === "/api/save") {
      if (!/^application\/json/.test(req.headers["content-type"] || "")) return send(res, 415, { error: "json only" });
      const { files = {}, tauri = false, ico = null } = JSON.parse(await readBody(req));
      const written = [];
      for (const [key, data] of Object.entries(files)) {
        const t = TARGETS[key];
        if (!t) return send(res, 400, { error: `unknown target ${key}` });
        const buf = key === "brand-config" ? Buffer.from(JSON.stringify(data, null, 2) + "\n") : pngFromDataUrl(data);
        fs.writeFileSync(path.join(ROOT, t[0]), buf);
        written.push(t[0]);
      }
      let log = "";
      if (tauri) {
        if (!fs.existsSync(path.join(ROOT, "brand/app-icon.png"))) return send(res, 400, { error: "no brand/app-icon.png yet" });
        const r = await tauriIcons();
        log = r.out;
        if (r.code !== 0) return send(res, 500, { error: "tauri icon failed", written, log });
        written.push("src-tauri/icons/*");
        // tauri icon downscales the 1024 master for every .ico frame; replace them with the page's
        // native renders so 16-32 px use the small mark, and 32x32.png (tray) with it too
        if (ico) {
          const frames = Object.entries(ico).map(([n, d]) => [Number(n), pngFromDataUrl(d)]).filter(([n]) => n >= 16 && n <= 256).sort((a, b) => a[0] - b[0]);
          fs.writeFileSync(path.join(ROOT, "src-tauri/icons/icon.ico"), icoFile(frames));
          const f32 = frames.find(([n]) => n === 32);
          if (f32) fs.writeFileSync(path.join(ROOT, "src-tauri/icons/32x32.png"), f32[1]);
          written.push("src-tauri/icons/icon.ico (native frames)");
        }
      }
      return send(res, 200, { written, log });
    }

    if (req.method === "GET") {
      let rel = decodeURIComponent(url.pathname).replace(/^\/+/, "");
      if (rel.endsWith("/") || rel === "brand") rel = rel.replace(/\/?$/, "/index.html");
      const abs = path.resolve(ROOT, rel);
      const ok = READ.some((d) => abs.startsWith(path.join(ROOT, d) + path.sep));
      if (!ok || !fs.existsSync(abs) || !fs.statSync(abs).isFile()) return send(res, 404, "not found", "text/plain");
      return send(res, 200, fs.readFileSync(abs), TYPES[path.extname(abs)] || "application/octet-stream");
    }

    send(res, 405, { error: "method not allowed" });
  } catch (e) {
    send(res, 500, { error: String(e.message || e) });
  }
});

server.listen(PORT, "127.0.0.1", () => console.log(`RigSweep brand studio: http://127.0.0.1:${PORT}/brand/`));
