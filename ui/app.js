const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (s) => document.querySelector(s);
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
const fmtBytes = (b) => {
  const neg = b < 0; b = Math.abs(b);
  if (b < 1024) return `${neg ? "-" : ""}${b} B`;
  const u = ["KB", "MB", "GB", "TB"]; let i = -1;
  do { b /= 1024; i++; } while (b >= 1024 && i < u.length - 1);
  return `${neg ? "-" : ""}${b.toFixed(b < 10 ? 1 : 0)} ${u[i]}`;
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
let snaps = [];            // saved Boost Report measurements
let avState = null;

function safeGet(k) { try { return localStorage.getItem(k); } catch { return null; } }
function safeSet(k, v) { try { localStorage.setItem(k, v); } catch {} }

// ---------- UI helpers ----------
function toast(msg, isErr = false, ms = 5000) {
  const el = $("#toast");
  el.textContent = msg;
  el.className = "toast" + (isErr ? " err" : "");
  clearTimeout(el._h);
  el._h = setTimeout(() => el.classList.add("hidden"), ms);
}

function modal(title, html, { okText = t("common.ok"), danger = false, cancel = true } = {}) {
  return new Promise((resolve) => {
    $("#modal-title").textContent = title;
    $("#modal-body").innerHTML = html;
    const ok = $("#modal-ok"), cn = $("#modal-cancel");
    ok.textContent = okText;
    ok.className = danger ? "danger" : "primary";
    cn.style.display = cancel ? "" : "none";
    $("#modal").classList.remove("hidden");
    const done = (v) => { $("#modal").classList.add("hidden"); ok.onclick = cn.onclick = null; resolve(v); };
    ok.onclick = () => done(true);
    cn.onclick = () => done(false);
  });
}

async function call(cmd, args) {
  try { return await invoke(cmd, args); }
  catch (e) { toast(`${cmd}: ${e}`, true, 8000); throw e; }
}

function busy(btn, on, label) {
  if (!btn) return;
  if (on) { btn._t = btn.textContent; btn.disabled = true; btn.textContent = label || t("common.working"); }
  else { btn.disabled = false; if (btn._t) btn.textContent = btn._t; }
}

// ---------- language ----------
const langSel = $("#lang-select");
langSel.innerHTML = Object.entries(LANGS).map(([k, v]) => `<option value="${k}">${v}</option>`).join("");
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
  if (name === "dashboard") loadDashboard();
  if (name === "cleanup") loadDev();
  if (name === "report") loadSnapshots();
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
    toast(msg, failed.length > 0, failed.length ? 15000 : 5000);
  } finally { busy(btn, false); }
  await loadState();
}

async function restoreIds(ids, btn) {
  busy(btn, true, t("tw.restoring"));
  try {
    const r = await call("restore_tweaks", { ids });
    const failed = r.filter((x) => !x.ok);
    toast(failed.length ? `${t("tw.restoreFail")}\n` + failed.map((f) => `• ${f.tweak_id}: ${f.messages.join("; ")}`).join("\n")
      : t("tw.restored", { n: r.length }), failed.length > 0, failed.length ? 12000 : 4000);
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
    toast(t("tw.checked", { c: s.checked, d: s.drifted, f: s.fixed }) + (s.errors.length ? `\n${s.errors.join("\n")}` : ""), s.errors.length > 0);
  } finally { busy(e.target, false); }
  loadState();
};

// ---------- dev cleanup ----------
const devKey = (p) => `${p.pid}:${p.start_time}`;

async function loadDev(btn) {
  busy(btn, true);
  try { dev = await call("get_dev_processes"); } finally { busy(btn, false); }
  // keep the user's ticks across rescans; pre-select stale ones the first time we see them
  const keys = new Set(dev.items.map(devKey));
  devSel = new Set([...devSel].filter((k) => keys.has(k)));
  if (!loadDev._seen) loadDev._seen = new Set();
  dev.items.forEach((p) => { if (p.stale && !loadDev._seen.has(devKey(p))) devSel.add(devKey(p)); loadDev._seen.add(devKey(p)); });
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
    const badges = reasons.map((r) => `<span class="badge ${r}">${esc(t("reason." + r, { o: p.owner }))}</span>`).join(" ");
    return `<tr class="${p.stale ? "stale" : ""} ${p.protected ? "locked" : ""}">
      <td>${p.protected ? "🔒" : `<input type="checkbox" data-dev="${esc(k)}" ${devSel.has(k) ? "checked" : ""} />`}</td>
      <td><b>${esc(p.label)}</b> <span class="sub">${esc(t("kind." + p.kind))}</span>
        <div class="sub" title="${esc(p.cmd)}">PID ${p.pid} · ${esc(p.parent_name || "—")} · ${p.tree_count} proc</div></td>
      <td>${esc(p.project || "—")}</td>
      <td class="num">${fmtAge(p.age_secs)}</td>
      <td class="num">${fmtBytes(p.tree_mem)}</td>
      <td>${p.ports.length ? p.ports.slice(0, 4).map((x) => `<code>${x}</code>`).join(" ") + (p.ports.length > 4 ? " …" : "") : "–"}</td>
      <td>${badges}</td>
    </tr>`;
  }).join("");
}

$("#clean-body").addEventListener("change", (e) => {
  const k = e.target.dataset.dev;
  if (k) e.target.checked ? devSel.add(k) : devSel.delete(k);
});
$("#btn-clean-scan").onclick = (e) => loadDev(e.target);
$("#btn-clean-stale").onclick = () => { dev?.items.filter((p) => p.stale).forEach((p) => devSel.add(devKey(p))); renderDev(); };
$("#btn-clean-end").onclick = async (e) => {
  const picked = (dev?.items || []).filter((p) => devSel.has(devKey(p)) && !p.protected);
  if (!picked.length) return toast(t("clean.none"));
  const serving = picked.filter((p) => p.ports.length).length;
  const agents = picked.filter((p) => p.kind === "agent").length;
  const html = `<p>${esc(t("clean.confirm.body"))}</p><ul>${picked.map((p) =>
    `<li><b>${esc(p.label)}</b> <span class="sub">${esc(p.project || "—")}${p.ports.length ? " · :" + p.ports.slice(0, 3).join(", :") : ""} · ${fmtBytes(p.tree_mem)} · PID ${p.pid} (+${p.tree_count - 1})</span></li>`).join("")}</ul>
    ${serving ? `<p class="warn-text">${esc(t("clean.serving.warn", { n: serving }))}</p>` : ""}
    ${agents ? `<p class="sub">💡 <code>claude --resume</code> / <code>codex resume</code></p>` : ""}`;
  if (!(await modal(t("clean.confirm.title", { n: picked.length }), html, { okText: t("clean.end"), danger: true }))) return;
  busy(e.target, true);
  try {
    const r = await call("kill_dev_processes", { targets: picked.map((p) => ({ pid: p.pid, start_time: p.start_time, label: p.label, project: p.project, ports: p.ports })) });
    let msg = t("clean.done", { n: r.result.ended.length, mem: fmtBytes(r.result.freed_mem) });
    if (r.result.failed.length) msg += `\n\n${t("clean.failed")}\n• ` + r.result.failed.join("\n• ");
    toast(msg + `\n${t("clean.seeReport")}`, r.result.failed.length > 0, 10000);
    reportPick = { before: r.before.id, after: r.after.id };
    picked.forEach((p) => devSel.delete(devKey(p)));
  } finally { busy(e.target, false); }
  loadDev();
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
  $("#report-body").innerHTML = `<table class="tbl report"><thead><tr><th></th>
    <th class="num">${esc(t("report.before"))}</th><th class="num">${esc(t("report.after"))}</th><th class="num">${esc(t("report.change"))}</th><th class="barcol"></th></tr></thead><tbody>
    ${METRICS.map(([k, f, lower]) => {
      const vb = metricVal(b.metrics, k), va = metricVal(a.metrics, k), d = va - vb;
      const better = d === 0 ? "" : (d < 0) === lower ? "good" : "bad";
      const max = Math.max(vb, va, 1);
      const pct = vb ? ` (${d > 0 ? "+" : ""}${((d / vb) * 100).toFixed(0)}%)` : "";
      return `<tr><td>${esc(t("m." + k))}</td><td class="num">${f(vb)}</td><td class="num">${f(va)}</td>
        <td class="num ${better}">${d > 0 ? "+" : ""}${f(d)}${pct}</td>
        <td class="barcol"><div class="bar b" style="width:${(vb / max) * 100}%"></div><div class="bar a ${better}" style="width:${(va / max) * 100}%"></div></td></tr>`;
    }).join("")}</tbody></table>`;
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
  toast(t("report.copied"));
};
$("#btn-clear-snaps").onclick = async () => {
  if (!(await modal(t("report.clear"), esc(t("report.clear.confirm")), { okText: t("report.clear"), danger: true }))) return;
  await call("clear_snapshots");
  reportPick = null;
  loadSnapshots();
};

// ---------- processes ----------
async function loadProcesses() {
  try { snapshot = await invoke("get_processes"); } catch (e) { return toast(`get_processes: ${e}`, true); }
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
      <td><b>${esc(p.name)}</b><div class="sub" title="${esc(p.exe)}">${esc(p.exe.length > 60 ? "…" + p.exe.slice(-58) : p.exe)}</div></td>
      <td class="num">${p.pid}</td>
      <td class="num ${cpuCls}">${p.cpu.toFixed(1)}%</td>
      <td class="num">${fmtBytes(p.mem)}</td>
      <td class="num">${diskOf(p) ? fmtBytes(diskOf(p)) : "–"}</td>
      <td>${svcs}</td>
      <td>${p.protected ? `<span class="sub">🔒</span>` : `<button class="small ghost" data-kill="${p.pid}" data-name="${esc(p.name)}">${esc(t("proc.end"))}</button>`}</td>
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
    const name = await call("kill_process", { pid: Number(el.dataset.kill) });
    toast(t("proc.ended", { name }));
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
    toast(res?.ok ? t("proc.disabled", { n: el.dataset.disable }) : t("proc.problem", { m: res?.messages.join("; ") }), !res?.ok, 8000);
    selected.add(id);
    await loadState();
    loadProcesses();
  }
});

// ---------- dashboard ----------
async function loadDashboard() {
  const [snap, hist, devr] = await Promise.all([
    invoke("get_processes").catch(() => null),
    invoke("get_history", { limit: 12 }).catch(() => []),
    invoke("get_dev_processes").catch(() => null),
  ]);
  const callouts = [];
  if (snap) {
    snapshot = snap;
    $("#t-cpu").textContent = `${snap.cpu_total.toFixed(0)}%`;
    $("#t-mem").textContent = `${fmtBytes(snap.mem_used)} / ${fmtBytes(snap.mem_total)}`;
    $("#t-procs").textContent = `${snap.process_count} / ${snap.service_running}`;
    const top = [...snap.processes].sort((a, b) => (b.cpu + diskOf(b) / 5e6) - (a.cpu + diskOf(a) / 5e6)).slice(0, 10);
    $("#dash-top").innerHTML = top.map((p) => `<tr><td><b>${esc(p.name)}</b>${p.services.length ? `<div class="sub">${esc(p.services.map((s) => s.name).join(", "))}</div>` : ""}</td>
      <td class="num">${p.cpu.toFixed(1)}%</td><td class="num">${fmtBytes(p.mem)}</td><td class="num">${diskOf(p) ? fmtBytes(diskOf(p)) : "–"}</td></tr>`).join("");
    const chrome = snap.processes.filter((p) => p.name.toLowerCase() === "chrome.exe");
    const cmem = chrome.reduce((n, p) => n + p.mem, 0);
    if (cmem > 1.5 * 1024 ** 3) callouts.push(`<div class="callout">${esc(t("dash.chrome", { n: chrome.length, mem: fmtBytes(cmem) }))} <button class="small ghost" data-go="settings">${esc(t("dash.chrome.go"))}</button></div>`);
  }
  if (devr?.stale_count) {
    dev = devr;
    callouts.unshift(`<div class="callout hot">${esc(t("dash.stale", { n: devr.stale_count, mem: fmtBytes(devr.stale_mem) }))} <button class="small" data-go="cleanup">${esc(t("dash.stale.go"))}</button></div>`);
  }
  $("#dash-callouts").innerHTML = callouts.join("");
  if (state) {
    $("#t-tweaks").textContent = `${state.tweaks.filter((x) => x.status.overall === "applied").length} / ${state.tweaks.length}`;
    $("#t-drift").textContent = state.tweaks.reduce((n, x) => n + x.drift_count, 0);
  }
  $("#dash-history").innerHTML = hist.length ? hist.map((h) =>
    `<li><span class="ts">${esc(h.ts)}</span>${h.result === "error" ? "⚠ " : ""}<b>${esc(h.action)}</b> ${esc(h.tweak_id || "")} <span class="sub">${esc(h.source)}</span></li>`).join("")
    : `<li class="sub">${esc(t("dash.empty"))}</li>`;
}
$("#btn-refresh-dash").onclick = async () => { await loadState(); loadDashboard(); };
$("#dash-callouts").addEventListener("click", (e) => { if (e.target.dataset.go) showTab(e.target.dataset.go); });

// ---------- history ----------
async function loadHistory() {
  const rows = await call("get_history", { limit: 500 });
  $("#backup-info").textContent = t("hist.info", { n: state?.backup_count ?? 0, dir: state?.data_dir ?? "" });
  $("#hist-body").innerHTML = rows.map((h) => `<tr>
    <td class="num">${esc(h.ts)}</td><td>${esc(h.source)}</td><td>${esc(h.tweak_id || "")}</td><td>${esc(h.action)}</td>
    <td style="color:${h.result === "ok" ? "var(--safe)" : "var(--caution)"}">${esc(h.result)}</td>
    <td class="sub">${esc(h.detail)}</td></tr>`).join("");
}

async function restoreAll() {
  if (!(await modal(t("hist.restoreTitle"), `<p>${esc(t("hist.restoreBody"))}</p>`, { okText: t("hist.restoreAll"), danger: true }))) return;
  const r = await call("restore_all");
  const failed = r.filter((x) => !x.ok);
  toast(failed.length ? t("hist.restoredErr", { n: failed.length }) : t("hist.restored", { n: r.length }), failed.length > 0);
  selected.clear();
  await loadState();
  loadHistory();
}
$("#btn-restore-all").onclick = restoreAll;

$("#btn-export").onclick = async () => {
  const path = await call("export_profile");
  toast(t("hist.exported", { p: path }), false, 8000);
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
  $("#s-idle").value = s.idle_hours ?? 2;
  $("#s-folders").value = s.dev_folders.join("\n");
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
    start_with_windows: $("#s-startup").checked,
    stay_in_tray: $("#s-tray").checked,
    watchdog_minutes: Math.max(1, Number($("#s-watchdog").value) || 30),
    idle_hours: Math.min(168, Math.max(1, Number($("#s-idle").value) || 2)),
    dev_folders: $("#s-folders").value.split(/\r?\n/).map((x) => x.trim()).filter(Boolean),
  };
  busy(e.target, true, t("set.saving"));
  try {
    const notes = await call("save_settings", { settings });
    toast([t("set.saved"), ...notes].join("\n"));
  } finally { busy(e.target, false); }
  await loadState();
  renderSettings();
};

// ---------- backend events ----------
listen("profile-changed", (e) => {
  const s = e.payload;
  if (s?.drifted) toast(t("tw.checked", { c: s.checked, d: s.drifted, f: s.fixed }));
  loadState();
});
listen("paused-changed", (e) => setPaused(e.payload));
listen("request-restore-all", () => { showTab("history"); restoreAll(); });

// ---------- boot ----------
(async () => {
  applyI18n();
  await loadState().catch(() => {});
  currentTab = safeGet("tab") || "dashboard";
  rerender();
})();
