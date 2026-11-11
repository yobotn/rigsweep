const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (s) => document.querySelector(s);
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
// In Hebrew / Arabic, keep "1.2 GB" reading left to right inside right-to-left text (LRI … PDI).
const ltr = (s) => (document.documentElement.dir === "rtl" ? `\u2066${s}\u2069` : s);
const fmtBytes = (b) => {
  const neg = b < 0; b = Math.abs(b);
  if (b < 1024) return ltr(`${neg ? "-" : ""}${b} B`);
  const u = ["KB", "MB", "GB", "TB"]; let i = -1;
  do { b /= 1024; i++; } while (b >= 1024 && i < u.length - 1);
  return ltr(`${neg ? "-" : ""}${b.toFixed(b < 10 ? 1 : 0)} ${u[i]}`);
};
const fmtAge = (s) => s < 3600 ? `${Math.max(1, Math.round(s / 60))}m` : s < 86400 ? `${(s / 3600).toFixed(1)}h` : `${(s / 86400).toFixed(1)}d`;

let state = null;          // StateView from backend
let selected = new Set();  // tweak ids checked in the UI
let collapsed = new Set(JSON.parse(safeGet("collapsed") || "[]"));
let snapshot = null;
let procSort = { key: "cpu", dir: -1 };
let procTimer = null;
let currentTab = "dashboard";
let dev = null;            // DevReport
let devSel = new Set();    // "pid:start_time"
const devUnticked = new Set(); // stale rows the user un-ticked on purpose: never re-tick those
let snaps = [];            // saved Boost Report measurements
let avState = null;

function safeGet(k) { try { return localStorage.getItem(k); } catch { return null; } }
function safeSet(k, v) { try { localStorage.setItem(k, v); } catch {} }

// ---------- UI helpers ----------
// toast(msg, kind): kind is "ok" | "info" | "warn" | "err" (true = "err", for older call sites).
// The first line reads as the title. Toasts stack, pause while hovered, and close on ×.
const TOAST_ICON = { ok: "✓", info: "i", warn: "!", err: "!" };
function toast(msg, kind = "info", ms) {
  kind = kind === true ? "err" : kind || "info";
  ms ??= kind === "err" ? 10000 : kind === "warn" ? 8000 : 4500;
  const box = $("#toasts");
  // the same message again (a refresh that keeps failing) replaces the old toast instead of piling up
  [...box.children].forEach((x) => { if (x._msg === msg) x.remove(); });
  while (box.children.length >= 4) box.firstElementChild.remove();
  const el = document.createElement("div");
  el.className = `toast ${kind}`;
  el._msg = msg;
  if (kind === "ok") msg = msg.replace(/^✅s*/u, ""); // the icon already says it
  if (kind === "err") el.setAttribute("role", "alert");
  el.innerHTML = `<span class="t-icon" aria-hidden="true">${TOAST_ICON[kind]}</span><div class="t-msg"></div><button class="t-close" aria-label="${esc(t("common.close"))}">×</button><i class="t-bar"></i>`;
  el.querySelector(".t-msg").textContent = msg;
  const bar = el.querySelector(".t-bar");
  bar.style.animationDuration = `${ms}ms`;
  const close = () => {
    if (el._closing) return;
    el._closing = true;
    el.classList.add("out");
    setTimeout(() => el.remove(), 180);
  };
  bar.addEventListener("animationend", close);
  el.querySelector(".t-close").onclick = close;
  box.appendChild(el);
}

function modal(title, html, { okText = t("common.ok"), danger = false, cancel = true } = {}) {
  return new Promise((resolve) => {
    const box = $("#modal"), ok = $("#modal-ok"), cn = $("#modal-cancel");
    const prev = document.activeElement;
    $("#modal-title").textContent = title;
    $("#modal-body").innerHTML = html;
    ok.textContent = okText;
    ok.className = danger ? "danger" : "primary";
    cn.style.display = cancel ? "" : "none";
    box.classList.remove("hidden");
    // Esc and a click outside cancel; a destructive dialog starts on Cancel, not on the red button
    const onKey = (e) => { if (e.key === "Escape" && cancel) { e.preventDefault(); done(false); } };
    const done = (v) => {
      box.classList.add("hidden");
      ok.onclick = cn.onclick = box.onclick = null;
      document.removeEventListener("keydown", onKey, true);
      prev?.focus?.();
      resolve(v);
    };
    ok.onclick = () => done(true);
    cn.onclick = () => done(false);
    box.onclick = (e) => { if (e.target === box && cancel) done(false); };
    document.addEventListener("keydown", onKey, true);
    (danger && cancel ? cn : ok).focus();
  });
}

async function call(cmd, args) {
  try { return await invoke(cmd, args); }
  catch (e) { console.error(cmd, e); toast(`${t("err.failed")}\n${e}`, "err"); throw e; }
}

function busy(btn, on, label) {
  if (!btn) return;
  if (on) { btn._t = btn.textContent; btn.disabled = true; btn.classList.add("is-busy"); btn.setAttribute("aria-busy", "true"); btn.textContent = label || t("common.working"); }
  else { btn.disabled = false; btn.classList.remove("is-busy"); btn.removeAttribute("aria-busy"); if (btn._t) btn.textContent = btn._t; }
}

// label for a history row: translated action and tweak name, raw values as the fallback
const tOr = (k, fb) => { const v = t(k); return v === k ? fb : v; };
const actLabel = (h) => tOr("act." + h.action, h.action);
const tweakName = (id) => (id && state?.tweaks.find((x) => x.id === id)?.name) || id || "";

// ---------- language ----------
initLang();
const langSel = $("#lang-select");
langSel.innerHTML = Object.entries(LANGS).filter(([k]) => I18N[k]).map(([k, v]) => `<option value="${k}">${v}</option>`).join("");
langSel.value = LANG;
langSel.onchange = () => { setLang(langSel.value); rerender(); };

function rerender() {
  applyI18n();
  $("#tw-hint").innerHTML = t("tw.hint", {
    safe: `<span class="risk safe">${esc(t("risk.safe"))}</span>`,
    mod: `<span class="risk moderate">${esc(t("risk.moderate"))}</span>`,
    caution: `<span class="risk caution">${esc(t("risk.caution"))}</span>`,
  });
  if (state) { setStatusPills(); renderTweaks(); }
  showTab(currentTab);
}

// ---------- tabs ----------
document.querySelectorAll("nav button").forEach((b) =>
  b.addEventListener("click", () => showTab(b.dataset.tab)));

function showTab(name) {
  if (!document.getElementById(`tab-${name}`)) name = "dashboard";
  currentTab = name;
  document.querySelectorAll("nav button").forEach((b) => b.classList.toggle("active", b.dataset.tab === name));
  document.querySelectorAll(".tab").forEach((s) => s.classList.toggle("active", s.id === `tab-${name}`));
  safeSet("tab", name);
  clearInterval(procTimer);
  if (name === "processes") { loadProcesses(); procTimer = setInterval(() => $("#proc-auto").checked && loadProcesses(), 3000); }
  if (name === "dashboard") { loadDashboard(); procTimer = setInterval(() => document.visibilityState === "visible" && loadDashboard(), 10000); }
  if (name === "cleanup") loadDev();
  if (name === "disk") { renderDiskAuto(); if (!disk) loadDisk(); }
  if (name === "browser") loadBrowsers();
  if (name === "report") loadSnapshots();
  if (name === "hardware") loadHardware();
  if (name === "history") loadHistory();
  if (name === "settings") renderSettings();
}

// ---------- state ----------
async function loadState() {
  state = await call("get_state");
  if (selected.size === 0) {
    // default selection: whatever is already in the profile, otherwise all safe tweaks
    const prof = state.tweaks.filter((x) => x.in_profile).map((x) => x.id);
    selected = new Set(prof.length ? prof : state.tweaks.filter((x) => x.risk === "safe").map((x) => x.id));
  }
  setStatusPills();
  renderTweaks();
}

function setStatusPills() {
  $("#admin-status").textContent = state.elevated ? t("admin.ok") : t("admin.bad");
  $("#admin-status").classList.toggle("bad", !state.elevated);
  setPaused(state.paused);
}

function setPaused(p) {
  if (state) state.paused = p;
  $("#watchdog-status").textContent = p ? t("watchdog.paused") : t("watchdog.every", { n: state?.settings.watchdog_minutes ?? 30 });
  $("#watchdog-status").classList.toggle("bad", p);
}

// ---------- tweaks ----------
function renderTweaks() {
  const list = $("#tweak-list");
  const byCat = {};
  state.tweaks.forEach((x) => (byCat[x.category] ||= []).push(x));
  list.innerHTML = state.categories.filter((c) => byCat[c.id]?.length).map((c) => {
    const items = byCat[c.id];
    const applied = items.filter((x) => x.status.overall === "applied").length;
    const caution = items.filter((x) => x.risk === "caution").length;
    return `
    <div class="cat ${collapsed.has(c.id) ? "collapsed" : ""}" data-cat="${esc(c.id)}">
      <div class="cat-head">
        <span class="caret">▾</span>
        <h2>${esc(c.name)}</h2>
        ${caution ? `<span class="risk caution">${esc(t("tw.attention", { n: caution }))}</span>` : ""}
        <span class="count">${esc(t("tw.count", { a: applied, n: items.length }))}</span>
        <button class="small ghost" data-cat-apply="${esc(c.id)}">${esc(t("tw.catApply"))}</button>
        <button class="small ghost" data-cat-restore="${esc(c.id)}">${esc(t("tw.catRestore"))}</button>
      </div>
      <div class="cat-body">
        <div class="cat-desc">${esc(c.description)}</div>
        ${items.map(renderTweak).join("")}
      </div>
    </div>`;
  }).join("");
}

function renderTweak(x) {
  const st = x.status.overall;
  const warnCls = x.risk === "caution" ? "warn" : "warn mod";
  const actions = x.status.actions.map((a) => {
    const s = a.status.state;
    const d = a.status.detail ? ` — ${esc(a.status.detail)}` : "";
    return `<li>${s === "applied" ? "✓" : s === "not_present" ? "–" : s === "error" ? "✗" : "○"} ${esc(a.target)}${d}</li>`;
  }).join("");
  return `
  <div class="tw" data-id="${esc(x.id)}">
    <input type="checkbox" class="sel" ${selected.has(x.id) ? "checked" : ""} ${st === "not_present" ? "disabled" : ""} />
    <div>
      <div class="name">${esc(x.name)}</div>
      <div class="desc">${esc(x.description)}</div>
      ${x.warning ? `<div class="${warnCls}">${x.risk === "caution" ? "⚠ " : ""}${esc(x.warning)}</div>` : ""}
      <details><summary>${esc(t("tw.changes", { n: x.status.actions.length }))}</summary><ul>${actions}</ul></details>
    </div>
    <div class="right">
      <span class="risk ${x.risk}">${esc(t("risk." + x.risk))}</span>
      <span class="st ${st}">${esc(t("st." + st))}</span>
      ${x.drift_count ? `<span class="st drift" title="${esc(t("tw.drift"))}">↻ ${x.drift_count}</span>` : ""}
      ${st !== "applied" && st !== "not_present" ? `<button class="small" data-apply="${esc(x.id)}">${esc(t("tw.apply"))}</button>` : ""}
      ${x.in_profile || st === "applied" || st === "partial" ? `<button class="small ghost" data-restore="${esc(x.id)}">${esc(t("tw.restore"))}</button>` : ""}
      ${x.custom ? `<button class="small ghost" data-remove="${esc(x.id)}">✕</button>` : ""}
    </div>
  </div>`;
}

$("#tweak-list").addEventListener("click", async (e) => {
  const el = e.target;
  const head = el.closest(".cat-head");
  if (el.matches("input.sel")) {
    const id = el.closest(".tw").dataset.id;
    el.checked ? selected.add(id) : selected.delete(id);
    return;
  }
  if (el.dataset.apply) return applyIds([el.dataset.apply], el);
  if (el.dataset.restore) return restoreIds([el.dataset.restore], el);
  if (el.dataset.remove) {
    if (!(await modal(t("tw.removeTitle"), esc(t("tw.removeBody")), { okText: t("tw.remove") }))) return;
    await call("remove_custom", { id: el.dataset.remove });
    selected.delete(el.dataset.remove);
    return loadState();
  }
  if (el.dataset.catApply) {
    const ids = state.tweaks.filter((x) => x.category === el.dataset.catApply && selected.has(x.id)).map((x) => x.id);
    if (!ids.length) return toast(t("tw.nothingSel"));
    return applyIds(ids, el);
  }
  if (el.dataset.catRestore) {
    const ids = state.tweaks.filter((x) => x.category === el.dataset.catRestore && (x.in_profile || x.status.overall !== "not_applied")).map((x) => x.id);
    if (!ids.length) return toast(t("tw.nothingRestore"));
    return restoreIds(ids, el);
  }
  if (head && el.tagName !== "BUTTON") {
    const cat = head.parentElement;
    cat.classList.toggle("collapsed");
    cat.classList.contains("collapsed") ? collapsed.add(cat.dataset.cat) : collapsed.delete(cat.dataset.cat);
    safeSet("collapsed", JSON.stringify([...collapsed]));
  }
});

async function applyIds(ids, btn) {
  const tweaks = ids.map((id) => state.tweaks.find((x) => x.id === id)).filter(Boolean)
    .filter((x) => x.status.overall !== "applied" && x.status.overall !== "not_present");
  if (!tweaks.length) return toast(t("tw.already"));
  const risky = tweaks.filter((x) => x.risk !== "safe");
  const caution = tweaks.filter((x) => x.risk === "caution");
  if (risky.length) {
    const html = `<p>${esc(t("tw.sideEffects", { n: tweaks.length }))}</p><ul>${risky.map((x) =>
      `<li><span class="risk ${x.risk}">${esc(t("risk." + x.risk))}</span> <b>${esc(x.name)}</b><br><span class="sub">${esc(x.warning || x.description)}</span></li>`).join("")}</ul>
      <p class="sub">${esc(t("tw.backedUp"))}</p>`;
    if (!(await modal(caution.length ? t("tw.attentionTitle") : t("tw.confirm"), html, { okText: t("tw.apply"), danger: caution.length > 0 }))) return;
  }
  busy(btn, true, t("tw.applying"));
  try {
    const r = await call("apply_tweaks", { ids: tweaks.map((x) => x.id) });
    const failed = r.results.filter((x) => !x.ok);
    let msg = t("tw.applied", { ok: r.results.length - failed.length, n: r.results.length });
    if (r.restore_point) msg += `\n${r.restore_point}`;
    if (failed.length) msg += `\n\n${t("tw.issues")}\n` + failed.map((f) => `• ${f.tweak_id}: ${f.messages.filter((m) => !m.startsWith("applied")).join("; ")}`).join("\n");
    toast(msg, failed.length ? "warn" : "ok", failed.length ? 15000 : 5000);
  } finally { busy(btn, false); }
  await loadState();
}

async function restoreIds(ids, btn) {
  busy(btn, true, t("tw.restoring"));
  try {
    const r = await call("restore_tweaks", { ids });
    const failed = r.filter((x) => !x.ok);
    toast(failed.length ? `${t("tw.restoreFail")}\n` + failed.map((f) => `• ${f.tweak_id}: ${f.messages.join("; ")}`).join("\n")
      : t("tw.restored", { n: r.length }), failed.length ? "warn" : "ok", failed.length ? 12000 : 4000);
  } finally { busy(btn, false); }
  await loadState();
}

$("#btn-apply-selected").onclick = (e) => applyIds([...selected], e.target);
$("#btn-select-safe").onclick = () => {
  state.tweaks.filter((x) => x.risk === "safe" && x.status.overall !== "not_present").forEach((x) => selected.add(x.id));
  renderTweaks();
};
$("#btn-reapply").onclick = async (e) => {
  busy(e.target, true, t("tw.verifying"));
  try {
    const s = await call("reapply_now");
    toast(t("tw.checked", { c: s.checked, d: s.drifted, f: s.fixed }) + (s.errors.length ? `\n${s.errors.join("\n")}` : ""), s.errors.length ? "warn" : "ok");
  } finally { busy(e.target, false); }
  loadState();
};

// ---------- dev cleanup ----------
const devKey = (p) => `${p.pid}:${p.start_time}`;
// ecosystem names are proper nouns, so only "agent" / "VM" are translated
const ECO = { node: "⬢ Node", python: "🐍 Python", java: "☕ Java", dotnet: "🟣 .NET", ruby: "💎 Ruby",
  php: "🐘 PHP", rust: "🦀 Rust", go: "🐹 Go" };
const ecoLabel = (p) => p.kind === "agent" ? t("kind.agent") : p.kind === "vm" ? t("kind.vm")
  : p.eco === "browser" ? t("kind.browser") : (ECO[p.eco] || p.eco);

async function loadDev(btn) {
  busy(btn, true);
  try { dev = await call("get_dev_processes"); } finally { busy(btn, false); }
  // keep the user's ticks across rescans, and tick every stale row, including ones that only
  // became stale after a later scan (idle is measured over time), unless the user un-ticked it
  const keys = new Set(dev.items.map(devKey));
  devSel = new Set([...devSel].filter((k) => keys.has(k)));
  [...devUnticked].forEach((k) => { if (!keys.has(k)) devUnticked.delete(k); });
  dev.items.forEach((p) => { if (p.stale && !p.protected && !devUnticked.has(devKey(p))) devSel.add(devKey(p)); });
  renderDev();
}

function renderDev() {
  if (!dev) return;
  $("#clean-summary").textContent = dev.items.length
    ? t("clean.summary", { n: dev.items.length, mem: fmtBytes(dev.total_mem), s: dev.stale_count, sm: fmtBytes(dev.stale_mem) }) : "";
  if (!dev.items.length) {
    $("#clean-body").innerHTML = `<tr><td colspan="7" class="loading">${esc(t("clean.none"))}</td></tr>`;
    return;
  }
  $("#clean-body").innerHTML = dev.items.map((p) => {
    const k = devKey(p);
    const reasons = p.reasons.length ? p.reasons : ["active"];
    const badges = reasons.map((r) => `<span class="badge ${r}">${esc(t("reason." + r, { o: p.owner, p: p.dup_of }))}</span>`).join(" ");
    return `<tr class="${p.stale ? "stale" : ""} ${p.protected ? "locked" : ""}">
      <td>${p.protected ? "🔒" : `<input type="checkbox" data-dev="${esc(k)}" ${devSel.has(k) ? "checked" : ""} />`}</td>
      <td><b>${esc(p.label)}</b> <span class="eco ${esc(p.eco)}">${esc(ecoLabel(p))}</span>
        <div class="sub" title="${esc(p.cmd)}">PID ${p.pid} · ${esc(p.parent_name || "—")} · ${esc(t("clean.procs", { n: p.tree_count }))}</div></td>
      <td title="${esc(p.project_path)}">${esc(p.project || "—")}</td>
      <td class="num">${p.age_secs ? fmtAge(p.age_secs) : "—"}</td>
      <td class="num">${p.tree_mem ? fmtBytes(p.tree_mem) : "—"}</td>
      <td>${p.ports.length ? p.ports.slice(0, 4).map((x) => `<code>${x}</code>`).join(" ") + (p.ports.length > 4 ? " …" : "") : "–"}</td>
      <td>${badges}</td>
    </tr>`;
  }).join("");
}

$("#clean-body").addEventListener("change", (e) => {
  const k = e.target.dataset.dev;
  if (!k) return;
  if (e.target.checked) { devSel.add(k); devUnticked.delete(k); } else { devSel.delete(k); devUnticked.add(k); }
});
$("#btn-clean-scan").onclick = (e) => loadDev(e.target);
$("#btn-clean-stale").onclick = async (e) => {
  await loadDev(e.target); // fresh scan, so rows that just went stale are included
  const stale = dev.items.filter((p) => p.stale && !p.protected);
  stale.forEach((p) => { devSel.add(devKey(p)); devUnticked.delete(devKey(p)); });
  renderDev();
  if (!stale.length) toast(t("clean.noStale"));
};
$("#btn-clean-end").onclick = async (e) => {
  const picked = (dev?.items || []).filter((p) => devSel.has(devKey(p)) && !p.protected);
  if (!picked.length) return toast(t("clean.none"));
  const serving = picked.filter((p) => p.ports.length).length;
  const agents = picked.filter((p) => p.kind === "agent").length;
  const html = `<p>${esc(t("clean.confirm.body"))}</p><ul>${picked.map((p) =>
    `<li><b>${esc(p.label)}</b> <span class="sub"><bdi dir="ltr">${esc(p.project_path || p.project || "—")}</bdi>${p.ports.length ? " · :" + p.ports.slice(0, 3).join(", :") : ""} · ${fmtBytes(p.tree_mem)} · PID ${p.pid} (+${p.tree_count - 1})</span></li>`).join("")}</ul>
    ${serving ? `<p class="warn-text">${esc(t("clean.serving.warn", { n: serving }))}</p>` : ""}
    ${picked.some((p) => p.kind === "vm") ? `<p class="warn-text">${esc(t("clean.vm.warn"))}</p>` : ""}
    ${agents ? `<p class="sub">💡 <code>claude --resume</code> / <code>codex resume</code></p>` : ""}`;
  if (!(await modal(t("clean.confirm.title", { n: picked.length }), html, { okText: t("clean.end"), danger: true }))) return;
  busy(e.target, true);
  const estimate = picked.reduce((n, p) => n + p.tree_mem, 0);
  startRecovery(live.at(-1)?.mem_used ?? 0, Date.now(), estimate, false);
  try {
    const r = await call("kill_dev_processes", { targets: picked.map((p) => ({ pid: p.pid, start_time: p.start_time, label: p.label, project: p.project, project_path: p.project_path, ports: p.ports })) });
    let msg = t("clean.done", { n: r.result.ended.length, mem: fmtBytes(r.result.freed_mem) });
    if (r.result.failed.length) msg += `\n\n${t("clean.failed")}\n• ` + r.result.failed.join("\n• ");
    toast(msg + `\n${t("clean.seeReport")}`, r.result.failed.length ? "warn" : "ok", 10000);
    reportPick = { before: r.before.id, after: r.after.id };
    if (r.result.mem_before && recovery) { recovery.before = r.result.mem_before; recovery.estimate = r.result.freed_mem; renderLive(); }
    picked.forEach((p) => devSel.delete(devKey(p)));
  } finally { busy(e.target, false); }
  loadDev();
};

// ---------- browser cleanup ----------
let bc = null;            // BrowserReport
let bcSel = new Set();    // "browser|profile|key"
let bcSeen = new Set();
const bcKey = (b, p, i) => `${b.id}|${p.dir}|${i.key}`;
const bcRows = () => (bc?.browsers || []).flatMap((b) => b.profiles.flatMap((p) => p.items.map((i) => ({ b, p, i, k: bcKey(b, p, i) }))));
const bcPreselect = (r) => r.i.tier === "safe" && r.i.bytes >= 1048576 && !r.b.running;

async function loadBrowsers(btn) {
  busy(btn, true, t("bc.scanning"));
  if (!bc) $("#bc-body").innerHTML = `<tr><td colspan="5" class="loading"><span class="spinner"></span> ${esc(t("bc.scanning"))}</td></tr>`;
  try { bc = await call("get_browser_caches"); } finally { busy(btn, false); }
  const rows = bcRows(), keys = new Set(rows.map((r) => r.k));
  bcSel = new Set([...bcSel].filter((k) => keys.has(k)));
  rows.forEach((r) => {
    if (r.b.running) bcSel.delete(r.k);
    else if (!bcSeen.has(r.k) && bcPreselect(r)) bcSel.add(r.k);
    bcSeen.add(r.k);
  });
  renderBrowsers();
}

function renderBrowsers() {
  if (!bc) return;
  if (!bc.browsers.length) {
    $("#bc-summary").textContent = "";
    $("#bc-body").innerHTML = `<tr><td colspan="5" class="loading">${esc(t("bc.none"))}</td></tr>`;
    return;
  }
  const picked = bcRows().filter((r) => bcSel.has(r.k)).reduce((a, r) => a + r.i.bytes, 0);
  $("#bc-summary").textContent = t("bc.summary", { safe: fmtBytes(bc.safe_total), site: fmtBytes(bc.site_total), sel: fmtBytes(picked) });
  const total = (p) => p.items.reduce((a, i) => a + i.bytes, 0);
  $("#bc-body").innerHTML = bc.browsers.map((b) => {
    const head = `<tr class="group"><td colspan="5"><b>${esc(b.name)}</b> <span class="sub">${fmtBytes(b.profiles.reduce((a, p) => a + total(p), 0))}</span>
      ${b.running ? ` <span class="badge idle">${esc(t("bc.running"))}</span>${b.id === "edge" ? `<div class="sub">${esc(t("bc.edgeHint"))}</div>` : ""}` : ""}</td></tr>`;
    const profs = [...b.profiles].sort((x, y) => (x.dir === "") - (y.dir === "") || total(y) - total(x));
    return head + profs.map((p) => p.items.map((i, n) => {
      const k = bcKey(b, p, i);
      const pname = p.dir === "" ? t("bc.shared") : p.name;
      const showDir = p.dir && p.dir !== "." && p.dir !== p.name;
      return `<tr class="${b.running ? "locked" : ""}">
        <td>${b.running ? "🔒" : `<input type="checkbox" data-bc="${esc(k)}" ${bcSel.has(k) ? "checked" : ""} />`}</td>
        <td>${n === 0 ? `<b>${esc(pname)}</b>${showDir ? `<div class="sub"><bdi dir="ltr">${esc(p.dir)}</bdi></div>` : ""}` : ""}</td>
        <td>${esc(t("bc.item." + i.key))}</td>
        <td><span class="risk ${i.tier === "safe" ? "safe" : "caution"}">${esc(t("bc.tier." + i.tier))}</span></td>
        <td class="num">${fmtBytes(i.bytes)}</td>
      </tr>`;
    }).join("")).join("");
  }).join("");
}

$("#bc-body").addEventListener("change", (e) => {
  const k = e.target.dataset.bc;
  if (k) { e.target.checked ? bcSel.add(k) : bcSel.delete(k); renderBrowsers(); }
});
$("#btn-bc-scan").onclick = (e) => loadBrowsers(e.target);
$("#btn-bc-safe").onclick = () => { bcRows().filter((r) => r.i.tier === "safe" && !r.b.running && r.i.bytes > 0).forEach((r) => bcSel.add(r.k)); renderBrowsers(); };
$("#btn-bc-clean").onclick = async (e) => {
  const picked = bcRows().filter((r) => bcSel.has(r.k) && !r.b.running);
  if (!picked.length) return toast(t("bc.nothing"));
  const size = picked.reduce((a, r) => a + r.i.bytes, 0);
  const site = picked.filter((r) => r.i.tier === "site");
  const html = `<p>${esc(t("bc.confirm.body"))}</p><ul>${picked.map((r) =>
    `<li><b>${esc(r.b.name)}</b> · ${esc(r.p.dir === "" ? t("bc.shared") : r.p.name)} · ${esc(t("bc.item." + r.i.key))} <span class="sub">${fmtBytes(r.i.bytes)}</span></li>`).join("")}</ul>
    ${site.length ? `<p class="warn-text">${esc(t("bc.site.warn"))}</p>` : ""}`;
  if (!(await modal(t("bc.confirm.title", { size: fmtBytes(size) }), html, { okText: t("bc.clean"), danger: site.length > 0 }))) return;
  busy(e.target, true);
  try {
    const r = await call("clean_browser_caches", { targets: picked.map((x) => ({ browser: x.b.id, profile: x.p.dir, key: x.i.key })) });
    let msg = t("bc.done", { mem: fmtBytes(r.freed) });
    if (r.locked_files) msg += "\n" + t("bc.locked", { n: r.locked_files });
    if (r.skipped_running.length) msg += "\n" + t("bc.skipped", { b: r.skipped_running.join(", ") });
    toast(msg, r.skipped_running.length ? "warn" : "ok", 9000);
  } finally { busy(e.target, false); }
  loadBrowsers();
};

// ---------- dev disk ----------
// Paths are only display data here: the backend re-verifies every (project, folder) or cache id.
function lines(v) { return v.split(/\r?\n/).map((x) => x.trim()).filter(Boolean); }
let disk = null;            // DiskReport
let diskSel = new Set();    // "c|id" or "p|path|name"
const diskSeen = new Set();
const cKey = (c) => `c|${c.id}`;
const aKey = (p, a) => `p|${p.path}|${a.name}`;
const diskRows = () => [
  ...(disk?.caches || []).filter((c) => !c.disabled && !c.busy && c.bytes > 0).map((c) => ({ k: cKey(c), bytes: c.bytes, c })),
  ...(disk?.projects || []).filter((p) => !p.busy).flatMap((p) => p.items.map((a) => ({ k: aKey(p, a), bytes: a.bytes, p, a }))),
];
const shortPath = (p) => disk?.home && p.toLowerCase().startsWith(disk.home.toLowerCase() + "\\") ? "~" + p.slice(disk.home.length) : p;
// auto-select rules (Dev Disk bar): stale = untouched for N days; caches only the "rebuilds itself" ones
const diskAuto = () => {
  const s = state?.settings || {};
  return { days: s.disk_stale_days ?? 30, build: s.disk_auto_build ?? true, deps: s.disk_auto_deps ?? true,
    caches: s.disk_auto_caches ?? true, min: (s.disk_auto_min_mb ?? 50) * 1048576 };
};
const isStale = (p) => !p.busy && p.idle_days >= diskAuto().days;
const diskSuggested = (r) => {
  const a = diskAuto();
  if (r.bytes < a.min && r.a?.eco !== "leftover") return false;
  if (r.c) return a.caches && r.c.tier === "safe";
  if (r.a.eco === "leftover") return true;
  // the backend already rules out OneDrive, pnpm and venvs; recheck staleness and kind here
  const venv = r.a.eco === "python" && r.a.tier === "deps";
  return isStale(r.p) && !r.p.onedrive && !r.a.pnpm && !venv && (r.a.tier === "deps" ? a.deps : a.build);
};
const DA_DAYS = [3, 7, 14, 30, 60, 90, 180, 365], DA_MIN = [0, 10, 50, 100, 500, 1024];
function renderDiskAuto() {
  const a = diskAuto();
  const days = DA_DAYS.includes(a.days) ? DA_DAYS : [...DA_DAYS, a.days].sort((x, y) => x - y);
  $("#da-days").innerHTML = days.map((d) => `<option value="${d}">${esc(t("disk.auto.dayN", { n: d }))}</option>`).join("");
  $("#da-days").value = a.days;
  const mb = a.min / 1048576, mins = DA_MIN.includes(mb) ? DA_MIN : [...DA_MIN, mb].sort((x, y) => x - y);
  $("#da-min").innerHTML = mins.map((m) => `<option value="${m}">${m ? fmtBytes(m * 1048576) : "–"}</option>`).join("");
  $("#da-min").value = mb;
  $("#da-build").checked = a.build; $("#da-deps").checked = a.deps; $("#da-caches").checked = a.caches;
}
$("#disk-auto").addEventListener("change", async () => {
  const s = await call("set_disk_auto", {
    days: Number($("#da-days").value), build: $("#da-build").checked, deps: $("#da-deps").checked,
    caches: $("#da-caches").checked, minMb: Number($("#da-min").value),
  });
  if (state) state.settings = s;
  // the rules changed: the selection becomes exactly what they pick
  diskSel = new Set(diskRows().filter(diskSuggested).map((r) => r.k));
  renderDisk();
});
const DISK_ECO = { node: "Node", rust: "Rust", java: "Java", dotnet: ".NET", python: "Python", browser: "", go: "Go", leftover: "" };

let diskLoading = null;     // the scan in flight: a second click or tab switch waits for it
function loadDisk() {
  if (!diskLoading) diskLoading = scanDisk().finally(() => { diskLoading = null; });
  return diskLoading;
}
async function scanDisk() {
  const btn = $("#btn-disk-scan");
  busy(btn, true, t("bc.scanning"));
  if (!disk) $("#disk-body").innerHTML = `<tr><td colspan="6" class="loading"><span class="spinner"></span> ${esc(t("disk.scanning"))}</td></tr>`;
  try { disk = await call("get_dev_disk"); } finally { busy(btn, false); }
  const rows = diskRows(), keys = new Set(rows.map((r) => r.k));
  diskSel = new Set([...diskSel].filter((k) => keys.has(k)));
  rows.forEach((r) => { if (!diskSeen.has(r.k) && diskSuggested(r)) diskSel.add(r.k); diskSeen.add(r.k); });
  renderDisk();
}

function renderDisk() {
  renderDiskAuto();
  if (!disk) return;
  const picked = diskRows().filter((r) => diskSel.has(r.k)).reduce((a, r) => a + r.bytes, 0);
  $("#disk-summary").textContent = t("disk.summary", {
    caches: fmtBytes(disk.cache_total), projects: fmtBytes(disk.project_total), n: disk.projects.length,
    stale: fmtBytes(disk.projects.filter(isStale).reduce((a, p) => a + p.items.reduce((b, i) => b + i.bytes, 0), 0)), sel: fmtBytes(picked),
  });
  const chk = (k, locked) => locked ? "🔒" : `<input type="checkbox" data-disk="${esc(k)}" ${diskSel.has(k) ? "checked" : ""} />`;
  const tier = (x) => `<span class="risk ${x === "safe" || x === "build" ? "safe" : "moderate"}">${esc(t("disk.tier." + x))}</span>`;
  let html = "";
  if (disk.caches.length) {
    html += `<tr class="group"><td colspan="6"><b>${esc(t("disk.caches"))}</b> <span class="sub">${fmtBytes(disk.cache_total)}</span></td></tr>`;
    html += disk.caches.map((c) => {
      const info = [];
      if (c.disabled) info.push(`<span class="badge">${esc(t("disk.disabled"))}</span>`);
      if (c.busy) info.push(`<span class="badge idle">${esc(t("disk.cacheBusy"))}</span>`);
      if (c.keep_newest) info.push(esc(t("disk.keepNewest", { total: fmtBytes(c.total) })));
      if (c.tool) info.push(esc(t("disk.tool", { c: c.tool })));
      return `<tr class="${c.disabled || c.busy ? "locked" : ""}" title="${esc(c.paths.join("\n"))}">
        <td>${c.disabled ? "🚫" : chk(cKey(c), c.busy || c.bytes === 0)}</td>
        <td>${esc(t("disk.cache." + c.id))}</td><td>${tier(c.tier)}</td>
        <td class="num">${fmtBytes(c.bytes)}</td><td class="sub">${info.join(" · ")}</td>
        <td><button class="small ghost" data-disk-toggle="${esc(c.id)}" data-on="${c.disabled ? 1 : 0}">${esc(t(c.disabled ? "disk.enable" : "disk.disable"))}</button></td>
      </tr>`;
    }).join("");
  }
  html += `<tr class="group"><td colspan="6"><b>${esc(t("disk.projects"))}</b> <span class="sub">${fmtBytes(disk.project_total)}</span></td></tr>`;
  if (!disk.projects.length) html += `<tr><td colspan="6" class="loading">${esc(t("disk.none", { roots: disk.roots.map(shortPath).join(", ") }))}</td></tr>`;
  html += disk.projects.map((p) => {
    const total = p.items.reduce((a, i) => a + i.bytes, 0);
    const badges = [
      p.busy ? `<span class="badge serving">${esc(t("disk.inUse"))}</span>` : "",
      isStale(p) ? `<span class="badge idle">${esc(t("disk.untouched", { d: p.idle_days }))}</span>`
        : `<span class="badge active">${esc(p.idle_days === 0 ? t("disk.today") : t("disk.recent", { d: p.idle_days }))}</span>`,
      p.onedrive ? `<span class="badge orphan">${esc(t("disk.onedrive"))}</span>` : "",
    ].join("");
    const head = `<tr class="group"><td></td><td colspan="2"><b title="${esc(p.path)}"><bdi dir="ltr">${esc(shortPath(p.path))}</bdi></b></td>
      <td class="num sub">${fmtBytes(total)}</td><td>${badges}</td>
      <td><button class="small ghost" data-disk-exclude="${esc(p.path)}">${esc(t("disk.exclude"))}</button></td></tr>`;
    return head + p.items.map((a) => {
      const note = a.eco === "leftover" ? t("disk.leftover") : a.pnpm ? t("disk.pnpm") : a.eco === "python" && a.tier === "deps" ? t("disk.venv") : "";
      return `<tr class="${p.busy ? "locked" : ""}">
        <td>${chk(aKey(p, a), p.busy)}</td><td><code>${esc(a.name)}</code> <span class="sub">${esc(DISK_ECO[a.eco] ?? a.eco)}</span></td>
        <td>${tier(a.tier)}</td><td class="num">${fmtBytes(a.bytes)}</td><td class="sub">${esc(note)}</td><td></td>
      </tr>`;
    }).join("");
  }).join("");
  $("#disk-body").innerHTML = html;
}

$("#disk-body").addEventListener("change", (e) => {
  const k = e.target.dataset.disk;
  if (k) { e.target.checked ? diskSel.add(k) : diskSel.delete(k); renderDisk(); }
});
$("#disk-body").addEventListener("click", async (e) => {
  const tg = e.target.closest("[data-disk-toggle]"), ex = e.target.closest("[data-disk-exclude]");
  if (tg) {
    const id = tg.dataset.diskToggle, enabled = tg.dataset.on === "1";
    state.settings = await call("set_disk_cache_enabled", { id, enabled });
    const c = disk.caches.find((x) => x.id === id);
    if (c) c.disabled = !enabled;
    if (!enabled) diskSel.delete(`c|${id}`);
    disk.cache_total = disk.caches.filter((x) => !x.disabled).reduce((a, x) => a + x.bytes, 0);
    renderDisk();
  } else if (ex) {
    const path = ex.dataset.diskExclude;
    if (!(await modal(t("disk.excludeConfirm", { p: shortPath(path) }), `<p>${esc(t("disk.excludeBody"))}</p>`, { okText: t("disk.exclude") }))) return;
    state.settings = await call("add_disk_exclude", { path });
    const low = path.toLowerCase();
    disk.projects = disk.projects.filter((p) => { const l = p.path.toLowerCase(); return l !== low && !l.startsWith(low + "\\"); });
    [...diskSel].filter((k) => k.startsWith("p|") && !diskRows().some((r) => r.k === k)).forEach((k) => diskSel.delete(k));
    toast(t("disk.excluded", { p: shortPath(path) }), "ok");
    renderDisk();
  }
});
$("#btn-disk-scan").onclick = () => loadDisk();
$("#btn-disk-suggest").onclick = () => { diskRows().filter(diskSuggested).forEach((r) => diskSel.add(r.k)); renderDisk(); };
$("#btn-disk-clean").onclick = async (e) => {
  const picked = diskRows().filter((r) => diskSel.has(r.k));
  if (!picked.length) return toast(t("disk.nothing"));
  const size = picked.reduce((a, r) => a + r.bytes, 0);
  const deps = picked.some((r) => r.a?.tier === "deps");
  const redl = picked.some((r) => r.c?.tier === "redownload");
  const od = picked.some((r) => r.p?.onedrive);
  const li = picked.map((r) => r.c
    ? `<li>${esc(t("disk.cache." + r.c.id))} <span class="sub">${fmtBytes(r.bytes)}</span></li>`
    : `<li>${esc(shortPath(r.p.path))}\\<b>${esc(r.a.name)}</b> <span class="sub">${fmtBytes(r.bytes)}</span></li>`).join("");
  const html = `<p>${esc(t("disk.confirm.body"))}</p><ul class="scroll-list">${li}</ul>
    ${deps ? `<p class="warn-text">${esc(t("disk.deps.warn"))}</p>` : ""}
    ${redl ? `<p class="warn-text">${esc(t("disk.redl.warn"))}</p>` : ""}
    ${od ? `<p class="warn-text">${esc(t("disk.od.warn"))}</p>` : ""}`;
  if (!(await modal(t("disk.confirm.title", { size: fmtBytes(size) }), html, { okText: t("disk.clean"), danger: true }))) return;
  busy(e.target, true, t("disk.cleaning"));
  try {
    const targets = picked.map((r) => r.c ? { kind: "cache", id: r.c.id, name: "" } : { kind: "project", id: r.p.path, name: r.a.name });
    const r = await call("clean_dev_disk", { targets });
    let msg = t("disk.done", { mem: fmtBytes(r.freed) });
    if (r.locked_files) msg += "\n" + t("disk.locked", { n: r.locked_files });
    if (r.skipped.length) msg += "\n" + t("disk.skipped") + "\n• " + r.skipped.map(shortPath).join("\n• ");
    toast(msg, r.skipped.length ? "warn" : "ok", 10000);
    picked.forEach((x) => diskSel.delete(x.k));
    // drop what's gone at once; the rescan below (slow on big trees) only refreshes the sizes
    const gone = new Set(r.done);
    disk.caches.forEach((c) => { if (gone.has(cKey(c))) { c.bytes = 0; c.total = 0; } });
    disk.caches = disk.caches.filter((c) => c.total > 0);
    disk.projects.forEach((p) => { p.items = p.items.filter((a) => !gone.has(aKey(p, a))); });
    disk.projects = disk.projects.filter((p) => p.items.length);
    const sum = (p) => p.items.reduce((a, i) => a + i.bytes, 0);
    disk.cache_total = disk.caches.filter((c) => !c.disabled).reduce((a, c) => a + c.bytes, 0);
    disk.project_total = disk.projects.reduce((a, p) => a + sum(p), 0);
    disk.stale_total = disk.projects.filter((p) => p.stale).reduce((a, p) => a + sum(p), 0);
    renderDisk();
  } finally { busy(e.target, false); }
  loadDisk();
};

// ---------- boost report ----------
let reportPick = null; // {before, after} snapshot ids
const METRICS = [
  // key, formatter, lower-is-better
  ["mem_used", fmtBytes, true],
  ["mem_free", fmtBytes, false],
  ["cpu_avg", (v) => `${v.toFixed(1)}%`, true],
  ["process_count", String, true],
  ["service_running", String, true],
  ["chrome_mem", fmtBytes, true],
  ["chrome_procs", String, true],
  ["dev_mem", fmtBytes, true],
  ["dev_procs", String, true],
];
const metricVal = (m, k) => k === "mem_free" ? m.mem_total - m.mem_used : m[k] ?? 0;
const snapLabel = (s) => `${s.ts} · ${t("label." + s.label) === "label." + s.label ? s.label : t("label." + s.label)}`;

async function loadSnapshots() {
  snaps = await call("get_snapshots");
  const opts = snaps.map((s) => `<option value="${s.id}">${esc(snapLabel(s))}</option>`).join("");
  $("#snap-before").innerHTML = opts;
  $("#snap-after").innerHTML = opts;
  if (snaps.length) {
    const base = snaps.find((s) => s.label === "baseline") || snaps[0];
    const pick = reportPick || { before: base.id, after: snaps[snaps.length - 1].id };
    $("#snap-before").value = pick.before;
    $("#snap-after").value = pick.after;
  }
  renderReport();
}

function currentPair() {
  const b = snaps.find((s) => s.id === Number($("#snap-before").value));
  const a = snaps.find((s) => s.id === Number($("#snap-after").value));
  return [b, a];
}

function renderReport() {
  const [b, a] = currentPair();
  const win = $("#report-win");
  if (!b || !a) {
    $("#report-body").innerHTML = `<div class="card loading">${esc(t("report.none"))}</div>`;
    win.classList.add("hidden");
    return;
  }
  $("#report-body").innerHTML = `<div class="card flush"><table class="tbl report"><thead><tr><th></th>
    <th class="num">${esc(t("report.before"))}</th><th class="num">${esc(t("report.after"))}</th><th class="num">${esc(t("report.change"))}</th><th class="barcol"></th></tr></thead><tbody>
    ${METRICS.map(([k, f, lower]) => {
      const vb = metricVal(b.metrics, k), va = metricVal(a.metrics, k), d = va - vb;
      const better = d === 0 ? "" : (d < 0) === lower ? "good" : "bad";
      const max = Math.max(vb, va, 1);
      const pct = vb ? ` (${d > 0 ? "+" : ""}${((d / vb) * 100).toFixed(0)}%)` : "";
      return `<tr><td>${esc(t("m." + k))}</td><td class="num">${f(vb)}</td><td class="num">${f(va)}</td>
        <td class="num ${better}">${d > 0 ? "+" : ""}${f(d)}${pct}</td>
        <td class="barcol"><div class="bar b" style="width:${(vb / max) * 100}%"></div><div class="bar a ${better}" style="width:${(va / max) * 100}%"></div></td></tr>`;
    }).join("")}</tbody></table></div>`;
  const freed = b.metrics.mem_used - a.metrics.mem_used;
  const fewer = b.metrics.process_count - a.metrics.process_count;
  win.classList.toggle("hidden", !(freed > 0 || fewer > 0));
  win.textContent = t("report.win", { mem: fmtBytes(Math.max(0, freed)), p: Math.max(0, fewer) });
}

function reportMarkdown() {
  const [b, a] = currentPair();
  if (!b || !a) return "";
  const rows = METRICS.map(([k, f]) => {
    const vb = metricVal(b.metrics, k), va = metricVal(a.metrics, k), d = va - vb;
    return `| ${t("m." + k)} | ${f(vb)} | ${f(va)} | ${d > 0 ? "+" : ""}${f(d)} |`;
  });
  return [`**RigSweep Boost Report** · ${snapLabel(b)} → ${snapLabel(a)}`, "",
    `| | ${t("report.before")} | ${t("report.after")} | ${t("report.change")} |`, "|---|---:|---:|---:|", ...rows].join("\n");
}

$("#snap-before").onchange = $("#snap-after").onchange = () => {
  reportPick = { before: Number($("#snap-before").value), after: Number($("#snap-after").value) };
  renderReport();
};
$("#btn-measure").onclick = async (e) => {
  busy(e.target, true);
  try {
    const s = await call("take_snapshot", { label: snaps.length ? "manual" : "baseline" });
    const base = reportPick?.before ?? (snaps.find((x) => x.label === "baseline") || snaps[0] || s).id;
    reportPick = { before: base, after: s.id };
  } finally { busy(e.target, false); }
  loadSnapshots();
};
$("#btn-copy-md").onclick = async () => {
  const md = reportMarkdown();
  if (!md) return toast(t("report.none"));
  try { await navigator.clipboard.writeText(md); }
  catch {
    const ta = Object.assign(document.createElement("textarea"), { value: md });
    document.body.appendChild(ta); ta.select(); document.execCommand("copy"); ta.remove();
  }
  toast(t("report.copied"), "ok");
};
$("#btn-clear-snaps").onclick = async () => {
  if (!(await modal(t("report.clear"), esc(t("report.clear.confirm")), { okText: t("report.clear"), danger: true }))) return;
  await call("clear_snapshots");
  reportPick = null;
  loadSnapshots();
};

// ---------- processes ----------
async function loadProcesses() {
  try { snapshot = await invoke("get_processes"); } catch (e) { return toast(`${t("err.failed")}\n${e}`, "err"); }
  renderProcesses();
}

function diskOf(p) { return p.disk_read + p.disk_write; }

function renderProcesses() {
  if (!snapshot) return;
  const q = $("#proc-filter").value.trim().toLowerCase();
  let rows = snapshot.processes.filter((p) => !q || p.name.toLowerCase().includes(q) || String(p.pid) === q ||
    p.services.some((s) => s.name.toLowerCase().includes(q) || s.display.toLowerCase().includes(q)));
  const k = procSort.key;
  rows.sort((a, b) => {
    const va = k === "disk" ? diskOf(a) : a[k], vb = k === "disk" ? diskOf(b) : b[k];
    return (typeof va === "string" ? va.localeCompare(vb) : va - vb) * procSort.dir;
  });
  rows = rows.slice(0, 300);
  $("#proc-body").innerHTML = rows.map((p) => {
    const cpuCls = p.cpu > 25 ? "vhot" : p.cpu > 5 ? "hot" : "";
    const svcs = p.services.map((s) => {
      const btn = s.protected ? `<span class="sub" title="${esc(t("proc.protected"))}">🔒</span>`
        : s.tweak_id ? `<button data-goto="${esc(s.tweak_id)}">${esc(t("proc.tweak"))}</button>`
        : `<button data-disable="${esc(s.name)}" data-display="${esc(s.display)}">${esc(t("proc.disable"))}</button>`;
      return `<span class="svc ${s.tweak_id ? "known" : ""}" title="${esc(s.display)}">${esc(s.name)} ${btn}</span>`;
    }).join("");
    return `<tr>
      <td><b>${esc(p.name)}</b><div class="sub" title="${esc(p.exe)}"><bdi dir="ltr">${esc(p.exe.length > 60 ? "…" + p.exe.slice(-58) : p.exe)}</bdi></div></td>
      <td class="num">${p.pid}</td>
      <td class="num ${cpuCls}">${p.cpu.toFixed(1)}%</td>
      <td class="num">${fmtBytes(p.mem)}</td>
      <td class="num">${diskOf(p) ? fmtBytes(diskOf(p)) : "–"}</td>
      <td>${svcs}</td>
      <td>${p.protected ? `<span class="sub">🔒</span>` : `<button class="small ghost" data-kill="${p.pid}" data-start="${p.start_time}" data-name="${esc(p.name)}">${esc(t("proc.end"))}</button>`}</td>
    </tr>`;
  }).join("");
}

$("#proc-filter").addEventListener("input", renderProcesses);
$("#btn-refresh-proc").onclick = loadProcesses;
document.querySelectorAll("#tab-processes th[data-sort]").forEach((th) => th.addEventListener("click", () => {
  const k = th.dataset.sort;
  procSort = { key: k, dir: procSort.key === k ? -procSort.dir : (k === "name" ? 1 : -1) };
  renderProcesses();
}));

$("#proc-body").addEventListener("click", async (e) => {
  const el = e.target;
  if (el.dataset.kill) {
    if (!(await modal(t("proc.endTitle"), `<p>${t("proc.endBody", { name: esc(el.dataset.name), pid: esc(el.dataset.kill) })}</p>`, { okText: t("proc.end"), danger: true }))) return;
    const name = await call("kill_process", { pid: Number(el.dataset.kill), startTime: Number(el.dataset.start) });
    toast(t("proc.ended", { name }), "ok");
    loadProcesses();
  } else if (el.dataset.goto) {
    showTab("tweaks");
    const node = document.querySelector(`.tw[data-id="${CSS.escape(el.dataset.goto)}"]`);
    if (node) { node.closest(".cat").classList.remove("collapsed"); node.scrollIntoView({ block: "center" }); node.style.outline = "2px solid var(--accent)"; setTimeout(() => (node.style.outline = ""), 2500); }
  } else if (el.dataset.disable) {
    const html = `<p>${t("proc.disableBody", { d: esc(el.dataset.display), n: esc(el.dataset.disable) })}</p>
      <p class="warn-text">${esc(t("proc.disableWarn"))}</p>`;
    if (!(await modal(t("proc.disableTitle"), html, { okText: t("proc.disable").replace("…", ""), danger: true }))) return;
    const id = await call("add_custom_service", { name: el.dataset.disable, display: el.dataset.display });
    const r = await call("apply_tweaks", { ids: [id] });
    const res = r.results[0];
    toast(res?.ok ? t("proc.disabled", { n: el.dataset.disable }) : t("proc.problem", { m: res?.messages.join("; ") }), res?.ok ? "ok" : "err", 8000);
    selected.add(id);
    await loadState();
    loadProcesses();
  }
});

// ---------- dashboard ----------
let dashLoading = false;
async function loadDashboard() {
  if (dashLoading) return;
  dashLoading = true;
  try { await renderDashboard(); } finally { dashLoading = false; }
}

// the watchdog logs a "checked, nothing drifted" row every few minutes: noise on the dashboard
const quietRow = (h) => h.action === "reapply" && h.result === "ok" && /drifted 0\b/.test(h.detail || "");

function feedItem(h) {
  const cls = h.result !== "ok" ? "err" : h.action === "drift" || h.action === "reapply" ? "info" : "";
  const name = tweakName(h.tweak_id);
  return `<li title="${esc(h.detail)}"><i class="dot ${cls}"></i><div class="what"><b>${esc(actLabel(h))}</b>${name ? ` · ${esc(name)}` : ""}
    <div class="sub">${esc(h.ts)} · ${esc(tOr("src." + h.source, h.source))}</div></div></li>`;
}

async function renderDashboard() {
  const [snap, hist, devr] = await Promise.all([
    invoke("get_processes").catch(() => null),
    invoke("get_history", { limit: 60 }).catch(() => []),
    invoke("get_dev_processes").catch(() => null),
  ]);
  const callouts = [];
  if (snap) {
    snapshot = snap;
    $("#t-procs").textContent = ltr(`${snap.process_count} / ${snap.service_running}`);
    const top = [...snap.processes].sort((a, b) => (b.cpu + diskOf(b) / 5e6) - (a.cpu + diskOf(a) / 5e6)).slice(0, 8);
    $("#dash-top").innerHTML = top.map((p) => `<tr><td><b>${esc(p.name)}</b>${p.services.length ? `<div class="sub">${esc(p.services.map((s) => s.name).join(", "))}</div>` : ""}</td>
      <td class="num ${p.cpu > 25 ? "vhot" : p.cpu > 5 ? "hot" : ""}">${p.cpu.toFixed(1)}%</td><td class="num">${fmtBytes(p.mem)}</td><td class="num">${diskOf(p) ? fmtBytes(diskOf(p)) : "–"}</td></tr>`).join("");
    const chrome = snap.processes.filter((p) => p.name.toLowerCase() === "chrome.exe");
    const cmem = chrome.reduce((n, p) => n + p.mem, 0);
    if (cmem > 1.5 * 1024 ** 3) callouts.push(`<div class="callout">${esc(t("dash.chrome", { n: chrome.length, mem: fmtBytes(cmem) }))} <button class="small ghost" data-go="settings">${esc(t("dash.chrome.go"))}</button></div>`);
  } else {
    $("#dash-top").innerHTML = `<tr><td colspan="4" class="loading">${esc(t("err.noData"))}</td></tr>`;
  }
  if (devr) dev = devr;
  if (devr?.stale_count) {
    callouts.unshift(`<div class="callout hot">${esc(t("dash.stale", { n: devr.stale_count, mem: fmtBytes(devr.stale_mem) }))} <button class="small" data-go="cleanup">${esc(t("dash.stale.go"))}</button></div>`);
  }
  $("#dash-callouts").innerHTML = callouts.join("");
  // drop the loading skeletons even if a call failed; "–" then stands for "no data"
  ["#t-procs", "#t-tweaks", "#t-drift"].forEach((id) => $(id).classList.remove("skel"));
  renderDashCharts();
  renderHeat();
  hardwareCallout();
  if (state) {
    const applied = state.tweaks.filter((x) => x.status.overall === "applied").length;
    $("#t-tweaks").textContent = ltr(`${applied} / ${state.tweaks.length}`);
    $("#t-tweaks-bar").style.width = `${state.tweaks.length ? (applied / state.tweaks.length) * 100 : 0}%`;
    $("#t-drift").textContent = state.tweaks.reduce((n, x) => n + x.drift_count, 0);
  }
  const rows = hist.filter((h) => !quietRow(h)).slice(0, 8);
  $("#dash-history").innerHTML = rows.length ? rows.map(feedItem).join("")
    : `<li class="empty-state">${esc(t("dash.empty"))}</li>`;
}

// CPU and RAM charts: the backend's 2-second samples, last 5 minutes, both on a 0–100 % scale
const memPct = (s) => (s.mem_total ? (s.mem_used / s.mem_total) * 100 : 0);
const CHARTS = {
  cpu: { val: (s) => Math.min(100, s.cpu), tip: (s) => `${s.cpu.toFixed(0)}%` },
  mem: { val: memPct, tip: (s) => `${memPct(s).toFixed(0)}% · ${fmtBytes(s.mem_used)}` },
};

function renderDashCharts() {
  const last = live.at(-1);
  if (last) {
    const cpus = live.map((s) => s.cpu);
    $("#t-cpu").textContent = `${last.cpu.toFixed(0)}%`;
    $("#t-cpu-sub").textContent = t("dash.cpuSub", { a: avg(cpus).toFixed(0), p: Math.max(...cpus).toFixed(0) });
    $("#t-mem").textContent = `${memPct(last).toFixed(0)}%`;
    $("#t-mem-sub").textContent = t("dash.memSub", { u: fmtBytes(last.mem_used), t: fmtBytes(last.mem_total) });
    $("#t-cpu").classList.remove("skel");
    $("#t-mem").classList.remove("skel");
  }
  Object.keys(CHARTS).forEach(drawChart);
}

function drawChart(id) {
  const plot = $(`#p-${id}`), svg = plot.querySelector("svg"), c = CHARTS[id];
  const enough = live.length >= 2;
  plot.querySelector(".chart-empty").classList.toggle("hidden", enough);
  if (!enough) { svg.innerHTML = ""; chartHover(id); return; }
  const W = 300, H = 100, t0 = live[0].t, span = Math.max(1, live.at(-1).t - t0);
  const pts = live.map((s) => `${(((s.t - t0) / span) * W).toFixed(1)},${(H - c.val(s)).toFixed(1)}`).join(" ");
  svg.innerHTML = `<line class="grid" x1="0" x2="${W}" y1="50" y2="50" /><line class="grid" x1="0" x2="${W}" y1="0.5" y2="0.5" />
    <polygon class="area" points="0,${H} ${pts} ${W},${H}" /><polyline class="line" points="${pts}" />`;
  chartHover(id);
}

// crosshair + tooltip on the sample nearest the pointer
function chartHover(id) {
  const plot = $(`#p-${id}`), f = plot._hover, c = CHARTS[id];
  const [cur, dot, tip] = [".chart-cursor", ".chart-dot", ".chart-tip"].map((q) => plot.querySelector(q));
  const off = f == null || live.length < 2;
  [cur, dot, tip].forEach((e) => e.classList.toggle("hidden", off));
  if (off) return;
  const t0 = live[0].t, span = Math.max(1, live.at(-1).t - t0), at = t0 + f * span;
  const s = live.reduce((b, x) => (Math.abs(x.t - at) < Math.abs(b.t - at) ? x : b));
  const x = ((s.t - t0) / span) * 100;
  cur.style.left = dot.style.left = `${x}%`;
  dot.style.top = `${100 - c.val(s)}%`;
  tip.style.left = `${Math.min(85, Math.max(15, x))}%`;
  const when = new Date(s.t).toLocaleTimeString(LANG, { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  tip.innerHTML = `<b>${esc(c.tip(s))}</b><span class="sub">${esc(when)}</span>`;
}

Object.keys(CHARTS).forEach((id) => {
  const plot = $(`#p-${id}`);
  plot.addEventListener("pointermove", (e) => {
    const r = plot.getBoundingClientRect();
    plot._hover = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
    chartHover(id);
  });
  plot.addEventListener("pointerleave", () => { plot._hover = null; chartHover(id); });
});

document.querySelectorAll(".tile.link").forEach((el) => {
  el.onclick = () => showTab(el.dataset.go);
  el.onkeydown = (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); showTab(el.dataset.go); } };
});

$("#btn-refresh-dash").onclick = async (e) => {
  busy(e.target, true);
  try { await loadState(); await loadDashboard(); } finally { busy(e.target, false); }
};
$("#dash-callouts").addEventListener("click", (e) => { if (e.target.dataset.go) showTab(e.target.dataset.go); });
$("#hw-callout").addEventListener("click", (e) => { if (e.target.dataset.go) showTab(e.target.dataset.go); });

// ---------- hardware ----------
let hw = null;              // hardware inventory (the backend caches it until Refresh)
const bench = {};           // drive letter -> speed test result, or "run" while it runs
const RAM_TYPES = { 20: "DDR", 21: "DDR2", 24: "DDR3", 26: "DDR4", 27: "LPDDR", 28: "LPDDR2", 29: "LPDDR3", 30: "LPDDR4", 34: "DDR5", 35: "LPDDR5" };
const kv = (rows) => `<dl class="kv">${rows.filter(([, v]) => v != null && v !== "").map(([k, v]) => `<dt>${esc(t(k))}</dt><dd>${v}</dd>`).join("")}</dl>`;
const nz = (v) => (v == null || v === 0 ? null : v);

// ok | warn | bad
function diskHealth(d) {
  const errs = (d.read_err || 0) + (d.write_err || 0);
  if ((d.health && d.health !== "Healthy") || errs > 0) return "bad";
  if ((d.wear ?? 0) >= 80 || (d.temp ?? 0) >= 70) return "warn";
  return "ok";
}

async function loadHardware(fresh = false) {
  const btn = $("#btn-hw-refresh");
  busy(btn, true);
  try { hw = await call("get_hardware", { fresh }); } catch { hw = hw || {}; } finally { busy(btn, false); }
  renderHardware();
}
$("#btn-hw-refresh").onclick = () => loadHardware(true);

// identical sticks collapse into one "8 × 4 GB" line
function groupModules(ram) {
  const g = new Map();
  for (const m of ram) {
    const k = [m.bytes, m.maker, m.part].join("|");
    g.set(k, { m, n: (g.get(k)?.n || 0) + 1 });
  }
  return [...g.values()].map(({ m, n }) => `${n > 1 ? `${n} × ` : ""}${fmtBytes(m.bytes)} <span class="sub">${esc([m.maker, m.part, n === 1 && m.slot].filter(Boolean).join(" · "))}</span>`).join("<br>");
}

function renderHardware() {
  if (!hw) return;
  const last = live.at(-1);
  const disks = hw.disks || [];
  const running = Object.values(bench).includes("run");
  $("#hw-disks").innerHTML = disks.length ? disks.map((d) => {
    const h = diskHealth(d);
    const smart = d.temp != null || d.wear != null || d.hours != null;
    const wear = d.wear != null ? `${100 - d.wear}% <div class="meter ${h === "ok" ? "" : h}"><i style="width:${100 - d.wear}%"></i></div>` : null;
    const benches = (d.letters || []).map((l) => {
      const b = bench[l];
      if (b === "run") return `<div class="bench sub">${esc(t("hw.speed.running", { d: l }))}</div>`;
      if (b) return `<div class="bench"><b>${esc(l)}</b> ${esc(t("hw.speed.result", { r: b.seq_read.toFixed(0), w: b.seq_write.toFixed(0), k: b.rnd_read.toFixed(1), iops: b.rnd_iops.toFixed(0) }))}</div>`;
      return "";
    }).join("");
    return `<div class="card">
      <h2><span>${d.media === "SSD" ? "⚡" : d.media === "HDD" ? "💿" : "💽"} ${esc(d.model || "Disk " + d.id)}</span><span class="health ${h}">${esc(t("hw.health." + h))}</span></h2>
      ${kv([
        ["hw.size", fmtBytes(d.bytes)],
        ["hw.type", esc([d.media !== "Unspecified" ? d.media : "", d.bus].filter(Boolean).join(" · "))],
        ["hw.drives", esc((d.letters || []).join(" "))],
        ["hw.life", wear],
        ["heat.title", nz(d.temp) != null ? `${fmtTemp(d.temp)}${nz(d.temp_max) ? ` <span class="sub">(${esc(t("hw.max"))} ${fmtTemp(d.temp_max)})</span>` : ""}` : null],
        ["hw.hours", d.hours != null ? esc(t("hw.hoursVal", { n: d.hours.toLocaleString(), d: Math.round(d.hours / 24).toLocaleString() })) : null],
        ["hw.errors", d.read_err != null || d.write_err != null ? String((d.read_err || 0) + (d.write_err || 0)) : null],
        ["hw.status", d.status && d.status !== "OK" ? esc(d.status) : null],
        ["hw.firmware", esc(d.firmware)],
      ])}
      ${smart ? "" : `<p class="sub">${esc(t("hw.noSmart"))}</p>`}
      ${h === "bad" ? `<p class="warn-text">${esc(t("hw.backup"))}</p>` : ""}
      ${benches}
      <div class="hw-actions">${(d.letters || []).map((l) => `<button class="small ghost" data-bench="${esc(l)}" ${running ? "disabled" : ""}>${esc(t("hw.speed", { d: l }))}</button>`).join("")}</div>
    </div>`;
  }).join("") : `<div class="card sub">${esc(t("hw.none"))}</div>`;

  const cards = [];
  for (const c of hw.cpu || []) cards.push(`<div class="card"><h2>🧮 ${esc(t("hw.cpu"))}</h2>${kv([
    ["hw.model", esc(c.name)],
    ["hw.cores", esc(t("hw.coresVal", { c: c.cores, t: c.threads }))],
    ["hw.clock", nz(c.max_mhz) ? `${(c.max_mhz / 1000).toFixed(2)} GHz` : null],
    ["hw.cache", [nz(c.l2_kb) && `L2 ${fmtBytes(c.l2_kb * 1024)}`, nz(c.l3_kb) && `L3 ${fmtBytes(c.l3_kb * 1024)}`].filter(Boolean).join(" · ")],
    ["hw.socket", esc(c.socket)],
    ["hw.virt", c.virt == null ? null : esc(t(c.virt ? "hw.on" : "hw.virtOff"))],
    ["heat.title", last?.cpu_temp != null ? `${fmtTemp(last.cpu_temp)} <span class="sub">${esc(t("heat.acpi"))}</span>` : null],
  ])}</div>`);
  for (const g of hw.gpu || []) cards.push(`<div class="card"><h2>🎮 ${esc(t("hw.gpu"))}</h2>${kv([
    ["hw.model", esc(g.name)],
    ["hw.vram", nz(g.vram) ? fmtBytes(g.vram) : null],
    ["hw.driver", esc([g.driver, g.driver_date && `(${g.driver_date})`].filter(Boolean).join(" "))],
    ["hw.display", nz(g.width) ? `${g.width}×${g.height}${nz(g.hz) ? ` @ ${g.hz} Hz` : ""}` : null],
    ["heat.title", last?.gpu_temp != null ? fmtTemp(last.gpu_temp) : null],
  ])}</div>`);
  const ram = hw.ram || [];
  if (ram.length) {
    const total = ram.reduce((a, m) => a + (m.bytes || 0), 0), m0 = ram[0];
    cards.push(`<div class="card"><h2>🧠 ${esc(t("hw.ram"))}</h2>${kv([
      ["hw.installed", fmtBytes(total)],
      ["hw.type", esc(RAM_TYPES[m0.type] || "")],
      ["hw.ramSpeed", nz(m0.mhz || m0.rated) ? `${m0.mhz || m0.rated} MT/s` : null],
      ["hw.slots", hw.ram_slots ? esc(t("hw.slotsVal", { u: ram.length, n: hw.ram_slots })) : null],
      ["hw.modules", groupModules(ram)],
    ])}</div>`);
  }
  const sys = hw.system || {}, bd = hw.board || {};
  const up = sys.boot ? (Date.now() - Date.parse(sys.boot)) / 1000 : null;
  cards.push(`<div class="card"><h2>💻 ${esc(t("hw.system"))}</h2>${kv([
    ["hw.model", esc([sys.maker, sys.model].filter(Boolean).join(" "))],
    ["hw.board", esc([bd.maker, bd.model].filter(Boolean).join(" "))],
    ["hw.bios", esc([bd.bios, bd.bios_date && `(${bd.bios_date})`].filter(Boolean).join(" "))],
    ["hw.windows", esc([sys.os, sys.build && `build ${sys.build}`].filter(Boolean).join(", "))],
    ["hw.uptime", up > 0 ? esc(fmtAge(up)) : null],
  ])}</div>`);
  for (const b of hw.battery || []) {
    if (!nz(b.full)) continue;
    const pct = nz(b.design) ? Math.min(100, Math.round((b.full / b.design) * 100)) : null;
    const cls = pct == null || pct >= 80 ? "" : pct >= 60 ? "warn" : "bad";
    cards.push(`<div class="card"><h2>🔋 ${esc(t("hw.battery"))}</h2>${kv([
      ["hw.batt.health", pct != null ? `${pct}% <div class="meter ${cls}"><i style="width:${pct}%"></i></div>` : null],
      ["hw.batt.full", `${(b.full / 1000).toFixed(1)} Wh`],
      ["hw.batt.design", nz(b.design) ? `${(b.design / 1000).toFixed(1)} Wh` : null],
    ])}</div>`);
  }
  $("#hw-parts").innerHTML = cards.join("");
}

$("#hw-disks").addEventListener("click", async (e) => {
  const d = e.target.closest("[data-bench]")?.dataset.bench;
  if (!d) return;
  if (!(await modal(t("hw.speed.title", { d }), `<p>${esc(t("hw.speed.note"))}</p>`, { okText: t("hw.speed.go") }))) return;
  bench[d] = "run";
  renderHardware();
  try { bench[d] = await call("disk_speed_test", { drive: d }); } catch { delete bench[d]; }
  renderHardware();
});

// a failing disk earns a dashboard warning; the inventory is read once, in the background
async function hardwareCallout() {
  try { hw = hw || await invoke("get_hardware", { fresh: false }); } catch { return; }
  const bad = (hw.disks || []).filter((d) => diskHealth(d) === "bad");
  $("#hw-callout").innerHTML = bad.map((d) => `<div class="callout hot">${esc(t("hw.diskCallout", { m: d.model, s: d.health }))} <button class="small" data-go="hardware">${esc(t("hw.go"))}</button></div>`).join("");
}

// ---------- history ----------
async function loadHistory() {
  const rows = await call("get_history", { limit: 500 });
  $("#backup-info").textContent = t("hist.info", { n: state?.backup_count ?? 0, dir: state?.data_dir ?? "" });
  $("#hist-body").innerHTML = rows.length ? rows.map((h) => `<tr>
    <td class="ts">${esc(h.ts)}</td><td>${esc(tOr("src." + h.source, h.source))}</td><td title="${esc(h.tweak_id || "")}">${esc(tweakName(h.tweak_id))}</td><td>${esc(actLabel(h))}</td>
    <td><span class="badge ${h.result === "ok" ? "active" : "orphan"}">${esc(h.result)}</span></td>
    <td class="sub">${esc(h.detail)}</td></tr>`).join("")
    : `<tr><td colspan="6" class="loading">${esc(t("dash.empty"))}</td></tr>`;
}

async function restoreAll() {
  if (!(await modal(t("hist.restoreTitle"), `<p>${esc(t("hist.restoreBody"))}</p>`, { okText: t("hist.restoreAll"), danger: true }))) return;
  const r = await call("restore_all");
  const failed = r.filter((x) => !x.ok);
  toast(failed.length ? t("hist.restoredErr", { n: failed.length }) : t("hist.restored", { n: r.length }), failed.length ? "warn" : "ok");
  selected.clear();
  await loadState();
  loadHistory();
}
$("#btn-restore-all").onclick = restoreAll;

$("#btn-export").onclick = async () => {
  const path = await call("export_profile");
  toast(t("hist.exported", { p: path }), "ok", 8000);
};
$("#file-import").addEventListener("change", async (e) => {
  const f = e.target.files[0];
  e.target.value = "";
  if (!f) return;
  const ids = await call("import_profile", { json: await f.text() });
  await loadState();
  if (!ids.length) return toast(t("hist.importNone"));
  if (await modal(t("hist.importTitle"), `<p>${esc(t("hist.importBody", { n: ids.length }))}</p>`, { okText: t("hist.review") })) {
    selected = new Set(ids);
    showTab("tweaks");
    renderTweaks();
    applyIds(ids, $("#btn-apply-selected"));
  }
});

// ---------- settings ----------
function renderSettings() {
  if (!state) return;
  const s = state.settings;
  $("#s-startup").checked = s.start_with_windows;
  $("#s-tray").checked = s.stay_in_tray;
  $("#s-watchdog").value = s.watchdog_minutes;
  $("#s-idle").value = s.idle_minutes ?? 120;
  $("#s-autokill").checked = !!s.auto_kill_stale;
  $("#s-badge").checked = s.badge_enabled ?? true;
  $("#s-badge-warn").value = s.badge_warn_gb ?? 1;
  $("#s-badge-alert").value = s.badge_alert_gb ?? 2;
  $("#s-folders").value = s.dev_folders.join("\n");
  $("#s-autoupdate").checked = !!s.auto_update;
  $("#s-disk-roots").value = (s.disk_roots || []).join("\n");
  $("#s-disk-excludes").value = (s.disk_excludes || []).join("\n");
  $("#s-disk-stale").value = s.disk_stale_days ?? 30;
  renderUpdate();
  $("#s-info").textContent = t("set.info", { s: state.startup_registered ? t("set.registered") : t("set.notRegistered"), d: state.data_dir });
  loadAv();
}

async function loadAv() {
  try { avState = await invoke("defender_self_status"); } catch { avState = null; }
  const st = $("#av-status"), btn = $("#btn-av");
  if (!avState) { st.textContent = ""; btn.style.display = "none"; return; }
  if (!avState.available) { st.textContent = t("av.na"); btn.style.display = "none"; return; }
  btn.style.display = "";
  st.innerHTML = `${esc(t("av.folder", { f: avState.folder }))}<br>${esc(avState.excluded ? t("av.on") : t("av.off"))}`;
  btn.textContent = avState.excluded ? t("av.disallow") : t("av.allow");
}

$("#btn-av").onclick = async (e) => {
  const allow = !avState?.excluded;
  if (allow && !(await modal(t("av.title"), t("av.confirm", { f: esc(avState.folder) }), { okText: t("av.allow") }))) return;
  busy(e.target, true);
  try { avState = await call("defender_self_allow", { allow }); } finally { busy(e.target, false); }
  loadAv();
};

$("#btn-chrome").onclick = async () => {
  const p = await call("open_chrome_helper");
  await modal(t("chrome.title"), `<p class="sub">${esc(t("chrome.opened", { p }))}</p>${t("chrome.steps")}`, { cancel: false });
};

$("#btn-save-settings").onclick = async (e) => {
  const settings = {
    ...state.settings,
    start_with_windows: $("#s-startup").checked,
    stay_in_tray: $("#s-tray").checked,
    watchdog_minutes: Math.max(1, Number($("#s-watchdog").value) || 30),
    idle_minutes: Math.min(10080, Math.max(1, Math.round(Number($("#s-idle").value)) || 120)),
    auto_kill_stale: $("#s-autokill").checked,
    badge_enabled: $("#s-badge").checked,
    badge_warn_gb: Number($("#s-badge-warn").value) || 1,
    badge_alert_gb: Number($("#s-badge-alert").value) || 2,
    auto_update: $("#s-autoupdate").checked,
    dev_folders: lines($("#s-folders").value),
    disk_roots: lines($("#s-disk-roots").value),
    disk_excludes: lines($("#s-disk-excludes").value),
    disk_disabled: state.settings.disk_disabled || [],
    disk_stale_days: Math.min(3650, Math.max(1, Math.round(Number($("#s-disk-stale").value)) || 30)),
  };
  busy(e.target, true, t("set.saving"));
  try {
    const notes = await call("save_settings", { settings });
    toast([t("set.saved"), ...notes].join("\n"), "ok");
  } finally { busy(e.target, false); }
  await loadState();
  renderSettings();
};

// ---------- updates ----------
// Nothing is checked unless the user clicks "Check now" or turns on automatic checks (off by
// default); the backend then emits "update-available". Installing always needs a click.
let update = null;      // {version, current, notes, date} when a newer version exists
let appVersion = "";
invoke("plugin:app|version").then((v) => { appVersion = v; renderUpdate(); }).catch(() => {});

function renderUpdate(status) {
  const pill = $("#update-pill");
  pill.classList.toggle("hidden", !update);
  if (update) pill.textContent = t("upd.pill", { v: update.version });
  const st = $("#upd-status"), notes = $("#upd-notes"), btn = $("#btn-upd-install");
  btn.classList.toggle("hidden", !update);
  notes.classList.toggle("hidden", !update?.notes);
  if (update?.notes) notes.innerHTML = `<b>${esc(t("upd.notes"))}</b><pre>${esc(update.notes)}</pre>`;
  if (status != null) st.textContent = status;
  else if (update) st.textContent = t("upd.available", { v: update.version, c: update.current });
  else if (appVersion) st.textContent = t("upd.current", { v: appVersion });
}

$("#btn-upd-check").onclick = async (e) => {
  busy(e.target, true, t("upd.checking"));
  try {
    update = await invoke("check_update");
    renderUpdate(update ? null : t("upd.latest", { v: appVersion }));
  } catch (err) {
    renderUpdate(t("upd.error", { m: String(err) }));
  } finally { busy(e.target, false); }
};

$("#btn-upd-install").onclick = async (e) => {
  if (!update) return;
  if (!(await modal(t("upd.confirm.title", { v: update.version }), `<p>${esc(t("upd.confirm.body"))}</p>`, { okText: t("upd.install") }))) return;
  busy(e.target, true, t("upd.downloading", { p: "" }));
  $("#btn-upd-check").disabled = true;
  try { await invoke("install_update"); }  // on success RigSweep exits while the installer runs
  catch (err) { renderUpdate(t("upd.error", { m: String(err) })); busy(e.target, false); $("#btn-upd-check").disabled = false; }
};

$("#update-pill").onclick = () => showTab("settings");

listen("update-available", (e) => { update = e.payload; renderUpdate(); });
listen("update-progress", (e) => {
  const { done, total } = e.payload;
  const p = total ? `${Math.floor((done / total) * 100)} %` : fmtBytes(done);
  $("#btn-upd-install").textContent = t("upd.downloading", { p });
});

// ---------- live meter ----------
// The backend samples CPU / RAM every 2 s for as long as RigSweep runs. After a cleanup the
// recovered RAM is measured from those samples: RAM in use right before the kill minus the
// lowest reading since, so it shows up within seconds instead of only in the Boost Report.
let live = [];          // [{t, cpu, mem_used, mem_total, cpu_temp?, gpu_temp?}]
let recovery = null;    // {before, since, estimate, auto, until}
const RECOVERY_MS = 20000;
const avg = (a) => a.length ? a.reduce((n, x) => n + x, 0) / a.length : 0;

function startRecovery(before, since, estimate, auto) {
  const cpuBefore = avg(live.filter((s) => s.t < since).slice(-3).map((s) => s.cpu));
  recovery = { before, since, estimate, auto, cpuBefore, until: Date.now() + RECOVERY_MS };
  renderLive();
}

function recoveredNow() {
  if (!recovery?.before) return null;
  const after = live.filter((s) => s.t >= recovery.since);
  if (!after.length) return null;
  const best = Math.max(0, ...after.map((s) => recovery.before - s.mem_used));
  // the kill itself costs CPU: judge CPU on samples from 4 s after it
  const settled = after.filter((s) => s.t >= recovery.since + 4000).map((s) => s.cpu);
  return { best, cpuAfter: settled.length >= 2 ? avg(settled) : null };
}

function renderLive() {
  const last = live.at(-1);
  if (last) {
    $("#live-mem").textContent = `${fmtBytes(last.mem_used)} / ${fmtBytes(last.mem_total)}`;
    $("#live-cpu").textContent = `${last.cpu.toFixed(0)}%`;
    if (currentTab === "dashboard") renderDashCharts();
  }
  renderSpark();
  renderHeat();
  const win = $("#live-win");
  if (!recovery) { win.className = "live-win hidden"; return; }
  const r = recoveredNow();
  const measuring = Date.now() < recovery.until;
  const tag = recovery.auto ? ` <span class="sub">· ${esc(t("live.auto"))}</span>` : "";
  if (!r || (measuring && r.best < 32 * 1048576)) {
    win.className = "live-win pending";
    win.innerHTML = esc(t("live.recovering"));
    return;
  }
  win.className = "live-win";
  const cpu = r.cpuAfter != null && recovery.cpuBefore
    ? `<span class="sub">${esc(t("live.cpu", { a: recovery.cpuBefore.toFixed(0), b: r.cpuAfter.toFixed(0) }))}</span>` : "";
  const note = r.best < 32 * 1048576 && recovery.estimate
    ? `<span class="sub">${esc(t("live.estimate", { mem: fmtBytes(recovery.estimate) }))}</span>` : "";
  win.innerHTML = `<span class="big">${esc(t("live.recovered", { mem: fmtBytes(r.best) }))}</span>${cpu}${tag}${note ? "<br>" + note : ""}`;
}

// ---------- heat ----------
// CPU temperature comes from the ACPI thermal zone (approximate, often missing on desktops),
// GPU temperature from the same counter Task Manager uses. Either may be null.
const HEAT = { warm: 80, hot: 90 };
let heatF = (() => { const s = safeGet("heatUnit"); return s ? s === "F" : LANG === "en" && navigator.language === "en-US"; })();
const fmtTemp = (c) => ltr(heatF ? `${Math.round(c * 9 / 5 + 32)} °F` : `${Math.round(c)} °C`);
const heatCls = (c) => (c >= HEAT.hot ? "vhot" : c >= HEAT.warm ? "hot" : "");
const heatOf = (s) => Math.max(s.cpu_temp ?? -1, s.gpu_temp ?? -1);

function heatParts(s) {
  const parts = [];
  if (s.cpu_temp != null) parts.push(`<span dir="ltr" class="${heatCls(s.cpu_temp)}">${esc(t("heat.cpu"))} ${fmtTemp(s.cpu_temp)}</span>`);
  if (s.gpu_temp != null) parts.push(`<span dir="ltr" class="${heatCls(s.gpu_temp)}">${esc(t("heat.gpu"))} ${fmtTemp(s.gpu_temp)}</span>`);
  return parts.join(" · ");
}

function renderHeat() {
  const last = live.at(-1);
  const has = live.some((s) => s.cpu_temp != null || s.gpu_temp != null);
  $("#live-heat-row").classList.toggle("hidden", !has);
  if (last && has) $("#live-heat").innerHTML = heatParts(last) || "–";
  if (currentTab !== "dashboard") return;
  const tile = $("#t-heat-tile");
  if (!last) return;
  $("#t-heat").classList.remove("skel");
  if (!has) {
    $("#t-heat").textContent = t("heat.na");
    $("#t-heat-sub").textContent = t("heat.naHint");
    $("#heat-spark").innerHTML = "";
    $("#heat-callout").innerHTML = "";
    tile.classList.add("na");
    return;
  }
  tile.classList.remove("na");
  $("#t-heat").innerHTML = heatParts(last) || "–";
  const peak = Math.max(...live.map(heatOf));
  $("#t-heat-sub").textContent = t("heat.peak", { t: fmtTemp(peak) }) + (last.cpu_temp != null ? " · " + t("heat.acpi") : "");
  // sparkline of the hotter sensor, scaled 30–100 °C so the warm/hot bands sit in the same place
  const pts = live.filter((s) => heatOf(s) > 0);
  if (pts.length >= 2) {
    const W = 300, H = 28, t0 = pts[0].t, span = Math.max(1, pts.at(-1).t - t0);
    const y = (c) => H - 1 - ((Math.min(100, Math.max(30, c)) - 30) / 70) * (H - 2);
    const line = pts.map((s) => `${(((s.t - t0) / span) * W).toFixed(1)},${y(heatOf(s)).toFixed(1)}`).join(" ");
    $("#heat-spark").innerHTML = `<line class="warn" x1="0" x2="${W}" y1="${y(HEAT.warm)}" y2="${y(HEAT.warm)}" /><polyline class="${heatCls(peak) || "ok"}" points="${line}" />`;
  }
  // warn only when it stays hot (last 3 samples), not on a single spike
  const recent = live.slice(-3);
  const hotNow = recent.length === 3 && recent.every((s) => heatOf(s) >= HEAT.hot);
  const which = last.cpu_temp != null && last.cpu_temp >= (last.gpu_temp ?? -1) ? t("heat.cpu") : t("heat.gpu");
  $("#heat-callout").innerHTML = hotNow
    ? `<div class="callout vhot">${esc(t("heat.hotCallout", { what: which, t: fmtTemp(heatOf(last)) }))} <button class="small" data-go="cleanup">${esc(t("dash.stale.go"))}</button></div>` : "";
}
const toggleHeatUnit = () => { heatF = !heatF; safeSet("heatUnit", heatF ? "F" : "C"); renderHeat(); };
$("#t-heat-tile").onclick = toggleHeatUnit;
$("#t-heat-tile").onkeydown = (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); toggleHeatUnit(); } };
$("#heat-callout").addEventListener("click", (e) => { if (e.target.dataset.go) showTab(e.target.dataset.go); });

function renderSpark() {
  const svg = $("#live-spark");
  if (live.length < 2) { svg.innerHTML = ""; return; }
  const W = 300, H = 48, t0 = live[0].t, span = Math.max(1, live.at(-1).t - t0);
  // RAM is zoomed to its recent range (a 2 GB drop on a 32 GB machine would look flat otherwise)
  const mems = live.map((s) => s.mem_used);
  const lo0 = Math.min(...mems), hi0 = Math.max(...mems), pad = Math.max(hi0 - lo0, 512 * 1048576) * 0.35;
  const lo = Math.max(0, lo0 - pad), range = hi0 + pad - lo;
  const x = (s) => ((s.t - t0) / span) * W;
  const ym = (s) => H - 2 - ((s.mem_used - lo) / range) * (H - 4);
  const yc = (s) => H - 2 - (Math.min(100, s.cpu) / 100) * (H - 4);
  const mem = live.map((s) => `${x(s).toFixed(1)},${ym(s).toFixed(1)}`).join(" ");
  const cpu = live.map((s) => `${x(s).toFixed(1)},${yc(s).toFixed(1)}`).join(" ");
  let mark = "";
  if (recovery && recovery.since >= t0) {
    const mx = (((recovery.since - t0) / span) * W).toFixed(1);
    mark = `<line class="mark" x1="${mx}" x2="${mx}" y1="0" y2="${H}" />`;
  }
  svg.innerHTML = `<polygon class="area" points="0,${H} ${mem} ${W},${H}" /><polyline class="cpu" points="${cpu}" /><polyline class="mem" points="${mem}" />${mark}`;
}

listen("live", (e) => {
  live.push(e.payload);
  if (live.length > 150) live.shift();
  renderLive();
});
invoke("get_live").then((h) => { if (h?.length) { live = h.concat(live.filter((s) => s.t > h.at(-1).t)); renderLive(); } }).catch(() => {});

// ---------- backend events ----------
listen("profile-changed", (e) => {
  const s = e.payload;
  if (s?.drifted) toast(t("tw.checked", { c: s.checked, d: s.drifted, f: s.fixed }));
  loadState();
});
listen("paused-changed", (e) => setPaused(e.payload));
listen("dev-autocleaned", (e) => {
  const r = e.payload;
  // the trees were ended a moment before this event; samples from just before it count
  if (r.mem_before) startRecovery(r.mem_before, Date.now() - 2500, r.freed_mem, true);
  toast(t("clean.auto.done", { n: r.ended.length, mem: fmtBytes(r.freed_mem) }) + "\n• " + r.ended.join("\n• "), "ok", 8000);
  if (currentTab === "cleanup") loadDev();
});
listen("request-restore-all", () => { showTab("history"); restoreAll(); });

// ---------- disclaimer ----------
async function ensureDisclaimer() {
  if (safeGet("disclaimer_v1") === "1") return;
  await modal(t("disc.title"), `<p>${esc(t("disc.body"))}</p>`, { okText: t("disc.accept"), cancel: false });
  safeSet("disclaimer_v1", "1");
}

// ---------- boot ----------
(async () => {
  applyI18n();
  await loadState().catch(() => {});
  currentTab = safeGet("tab") || "dashboard";
  rerender();
  ensureDisclaimer();
  // the heat tile waits for the first live sample; never let a skeleton spin forever
  setTimeout(() => document.querySelectorAll(".skel").forEach((el) => el.classList.remove("skel")), 8000);
})();
