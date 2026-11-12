// Demo backend for README screenshots. Loaded before i18n.js / app.js in a COPY of ui/ by
// make-screenshots.ps1, never shipped. All data below is made up: no real machine, paths or names.
(() => {
  const q = new URLSearchParams(location.search);
  const lang = q.get("lang") || "en";
  try {
    localStorage.setItem("lang", lang);
    localStorage.setItem("tab", q.get("tab") || "cleanup");
    localStorage.setItem("disclaimer_v1", "1");
  } catch {}

  const MB = 1048576, GB = 1024 * MB, H = 3600;
  const row = (o) => ({
    start_time: 1700000000 + o.pid, name: "node.exe", kind: "runtime", eco: "node", project: "", cmd: "",
    parent_pid: 0, parent_name: "", quiet_secs: 0, cpu: 0, tree_count: 1, ports: [], reasons: [],
    stale: false, protected: false, owner: "", dup_of: 0, ...o,
  });
  const items = [
    row({ pid: 18244, label: "@playwright/mcp", tree_count: 14, age_secs: 7.4 * H, tree_mem: 1.18 * GB, reasons: ["mcp", "duplicate", "orphan"], dup_of: 26112, stale: true, parent_name: "" }),
    row({ pid: 9120, label: "Gradle daemon", name: "java.exe", eco: "java", project: "android-app", tree_count: 2, age_secs: 26 * H, tree_mem: 1.21 * GB, reasons: ["idle", "daemon"], stale: true }),
    row({ pid: 22508, label: "Playwright Chromium (headless)", name: "chrome.exe", eco: "browser", tree_count: 9, age_secs: 5.1 * H, tree_mem: 734 * MB, reasons: ["orphan"], stale: true }),
    row({ pid: 15032, label: "vite", project: "shop-web", tree_count: 3, age_secs: 9.2 * H, tree_mem: 612 * MB, ports: [5173], reasons: ["orphan", "serving"], stale: true }),
    row({ pid: 7764, label: "@modelcontextprotocol/server-filesystem", tree_count: 2, age_secs: 7.4 * H, tree_mem: 96 * MB, reasons: ["mcp", "orphan"], stale: true }),
    row({ pid: 30112, label: "WSL 2 / Docker", name: "vmmemWSL", kind: "vm", eco: "wsl", age_secs: 0, tree_mem: 2.1 * GB }),
    row({ pid: 11408, label: "next", project: "dashboard", tree_count: 4, age_secs: 3.1 * H, tree_mem: 884 * MB, ports: [3000], reasons: ["idle", "serving"], parent_name: "pwsh.exe" }),
    row({ pid: 25690, label: "claude", name: "claude.exe", kind: "agent", eco: "agent", project: "shop-web", tree_count: 6, age_secs: 1.6 * H, tree_mem: 412 * MB, parent_name: "pwsh.exe" }),
    row({ pid: 4428, label: "codex", name: "codex.exe", kind: "agent", eco: "agent", project: "infra", tree_count: 3, age_secs: 4.8 * H, tree_mem: 268 * MB, reasons: ["idle"], parent_name: "pwsh.exe" }),
    row({ pid: 19876, label: "uvicorn", name: "python.exe", eco: "python", project: "api", tree_count: 2, age_secs: 2.2 * H, tree_mem: 154 * MB, ports: [8000], reasons: ["serving"], parent_name: "pwsh.exe" }),
    row({ pid: 13300, label: "typescript", project: "shop-web", tree_count: 2, age_secs: 1.6 * H, tree_mem: 318 * MB, reasons: ["owned"], protected: true, owner: "Code.exe", parent_name: "Code.exe" }),
  ].map((r) => ({ ...r, tree_mem: Math.round(r.tree_mem) }));
  const dev = {
    items,
    total_mem: items.reduce((n, i) => n + i.tree_mem, 0),
    stale_mem: items.filter((i) => i.stale).reduce((n, i) => n + i.tree_mem, 0),
    stale_count: items.filter((i) => i.stale).length,
  };

  // 5 minutes of live samples: a busy machine, then a sweep 70 % of the way in
  const TOTAL = 32 * GB, now = Date.now(), n = 150, sweepAt = 105;
  let seed = 7;
  const rnd = () => ((seed = (seed * 16807) % 2147483647) / 2147483647) - 0.5;
  const samples = [];
  for (let i = 0; i < n; i++) {
    const after = i >= sweepAt;
    const drop = after ? Math.min(1, (i - sweepAt + 1) / 2) * 2.62 * GB : 0;
    samples.push({
      t: now - (n - 1 - i) * 2000,
      cpu: Math.max(2, (after ? 11 : 34) + rnd() * 10),
      mem_used: Math.round(25.1 * GB + i * 0.002 * GB - drop + rnd() * 0.06 * GB),
      mem_total: TOTAL,
      // the CPU cools off a little after the sweep
      cpu_temp: Math.round((after ? 64 : 78) + rnd() * 4),
      gpu_temp: Math.round(52 + rnd() * 3),
    });
  }
  window.__DEMO__ = { sweepT: samples[sweepAt].t - 1, before: samples[sweepAt - 1].mem_used, estimate: 2.6 * GB };

  // Tweaks: the real catalog (catalog.js is written by make-screenshots.ps1), with a typical profile:
  // most safe tweaks applied, a few left for later, nothing that breaks a feature.
  const cat = window.__CATALOG__ || { categories: [], tweaks: [] };
  const later = new Set(["dev.power", "search.indexer", "updates.no_edge"]);
  const drift = { "telemetry.diagtrack": 3, "telemetry.tasks": 1, "updates.delivery_p2p": 2 };
  const tweaks = cat.tweaks.map((x) => {
    const on = x.risk === "safe" && !later.has(x.id);
    return {
      ...x, in_profile: on, drift_count: drift[x.id] || 0,
      status: { overall: on ? "applied" : "not_applied",
        actions: x.actions.map((a) => ({ target: actionTarget(a), status: { state: on ? "applied" : "not_applied" } })) },
    };
  });
  function actionTarget(a) {
    if (a.type === "service") return `service ${a.name} -> ${a.start[0].toUpperCase()}${a.start.slice(1)}`;
    if (a.type === "task") return `task ${a.path} -> disabled`;
    if (a.type === "registry") return `${a.hive}\\${a.path}\\${a.value} = ${JSON.stringify(a.data)}`;
    if (a.type === "power_plan") return `power plan -> ${a.scheme}`;
    if (a.type === "defender_exclusion") return a.use_dev_folders ? "defender exclusions: dev folders" : `defender exclusions: ${(a.processes || []).join(", ")}`;
    return a.type;
  }

  // Processes (dashboard + analyzer)
  const proc = (o) => ({ start_time: 1700000000 + o.pid, disk_read: 0, disk_write: 0, services: [], protected: false, ...o, mem: Math.round(o.mem) });
  const svc = (name, display, protectedSvc = false) => ({ name, display, protected: protectedSvc, tweak_id: null });
  const processes = [
    proc({ pid: 14220, name: "chrome.exe", exe: "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe", cpu: 18.4, mem: 1.1 * GB }),
    proc({ pid: 9876, name: "Code.exe", exe: "C:\\Program Files\\Microsoft VS Code\\Code.exe", cpu: 9.7, mem: 1.2 * GB, disk_read: 3.1 * MB }),
    proc({ pid: 18244, name: "node.exe", exe: "C:\\Program Files\\nodejs\\node.exe", cpu: 7.2, mem: 612 * MB }),
    proc({ pid: 30112, name: "vmmemWSL", exe: "", cpu: 6.1, mem: 2.1 * GB, protected: true }),
    proc({ pid: 25690, name: "claude.exe", exe: "C:\\Users\\dev\\.local\\bin\\claude.exe", cpu: 4.8, mem: 412 * MB }),
    proc({ pid: 5120, name: "Docker Desktop.exe", exe: "C:\\Program Files\\Docker\\Docker\\Docker Desktop.exe", cpu: 2.3, mem: 388 * MB }),
    proc({ pid: 4012, name: "MsMpEng.exe", exe: "C:\\ProgramData\\Microsoft\\Windows Defender\\Platform\\MsMpEng.exe", cpu: 3.9, mem: 296 * MB, disk_read: 9.4 * MB, protected: true }),
    proc({ pid: 1532, name: "svchost.exe", exe: "C:\\Windows\\System32\\svchost.exe", cpu: 1.4, mem: 64 * MB, protected: true,
      services: [svc("DiagTrack", "Connected User Experiences and Telemetry"), svc("dmwappushservice", "Device Management WAP Push")] }),
    proc({ pid: 2264, name: "SearchIndexer.exe", exe: "C:\\Windows\\System32\\SearchIndexer.exe", cpu: 2.6, mem: 92 * MB, disk_write: 4.2 * MB, services: [svc("WSearch", "Windows Search")] }),
    proc({ pid: 7788, name: "Teams.exe", exe: "C:\\Program Files\\WindowsApps\\MSTeams\\ms-teams.exe", cpu: 1.1, mem: 540 * MB }),
  ];
  const snapshot = { processes, process_count: 286, service_running: 97 };

  // Startup: typical third-party autostarts, none of them from a real machine
  const st = (o) => ({ location: "", scope: "machine", triggers: [], running: false, hardware: false, tweak_id: null, ...o });
  const startup = [
    st({ kind: "service", key: "VpnAgent", name: "Corporate VPN Agent", exe: "C:\\Program Files\\VPN Client\\vpnagent.exe", command: "\"C:\\Program Files\\VPN Client\\vpnagent.exe\" --service", running: true }),
    st({ kind: "service", key: "PrinterStatusSvc", name: "Printer Status Monitor", exe: "C:\\Program Files\\Printer Tools\\statusmon.exe", command: "\"C:\\Program Files\\Printer Tools\\statusmon.exe\" /service", running: true }),
    st({ kind: "service", key: "OemAudioService", name: "OEM Audio Effects Service", exe: "C:\\Windows\\System32\\DriverStore\\FileRepository\\oemaudio.inf\\AudioFx.exe", command: "C:\\Windows\\System32\\DriverStore\\FileRepository\\oemaudio.inf\\AudioFx.exe", running: true, hardware: true }),
    st({ kind: "service", key: "MongoDB", name: "MongoDB Server (MongoDB)", exe: "C:\\Program Files\\MongoDB\\Server\\7.0\\bin\\mongod.exe", command: "\"C:\\Program Files\\MongoDB\\Server\\7.0\\bin\\mongod.exe\" --service", running: true }),
    st({ kind: "task", key: "\\ZoomUpdateTask", name: "ZoomUpdateTask", exe: "C:\\Program Files\\Zoom\\bin\\Zoom.exe", command: "C:\\Program Files\\Zoom\\bin\\Zoom.exe --update", triggers: ["schedule"] }),
    st({ kind: "task", key: "\\GoogleUpdaterTask", name: "GoogleUpdaterTaskSystem", exe: "C:\\Program Files (x86)\\Google\\GoogleUpdater\\updater.exe", command: "C:\\Program Files (x86)\\Google\\GoogleUpdater\\updater.exe --wake", triggers: ["logon", "schedule"] }),
    st({ kind: "run", key: "HKCU|Run|Spotify", name: "Spotify", exe: "C:\\Users\\dev\\AppData\\Roaming\\Spotify\\Spotify.exe", command: "C:\\Users\\dev\\AppData\\Roaming\\Spotify\\Spotify.exe --autostart --minimized", scope: "user" }),
    st({ kind: "run", key: "HKLM|Run|Docker Desktop", name: "Docker Desktop", exe: "C:\\Program Files\\Docker\\Docker\\Docker Desktop.exe", command: "\"C:\\Program Files\\Docker\\Docker\\Docker Desktop.exe\" -Autostart" }),
  ];

  // App icons: plain lettered tiles (no real icons are bundled with the demo)
  const ICON_COLORS = ["#4f8cff", "#34c38f", "#f1b44c", "#f46a6a", "#b392f0", "#fb923c", "#22b8cf"];
  const icon = (p) => {
    const base = p.split("\\").pop().replace(/\.exe$/i, "");
    const c = ICON_COLORS[[...base].reduce((n, ch) => n + ch.charCodeAt(0), 0) % ICON_COLORS.length];
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect width="32" height="32" rx="7" fill="${c}"/><text x="16" y="22" font-family="Segoe UI,Arial" font-size="17" font-weight="700" fill="#fff" text-anchor="middle">${base[0].toUpperCase()}</text></svg>`;
    return "data:image/svg+xml;base64," + btoa(svg);
  };

  const ago = (min) => { const d = new Date(Date.now() - min * 60000); const z = (n) => String(n).padStart(2, "0"); return `${d.getFullYear()}-${z(d.getMonth() + 1)}-${z(d.getDate())} ${z(d.getHours())}:${z(d.getMinutes())}:${z(d.getSeconds())}`; };
  const history = [
    { id: 9, ts: ago(4), source: "gui", tweak_id: "custom.svc.adobearmservice", action: "apply", result: "ok", detail: "" },
    { id: 8, ts: ago(12), source: "gui", tweak_id: null, action: "dev_cleanup", result: "ok", detail: "ended 5 trees" },
    { id: 7, ts: ago(48), source: "watchdog", tweak_id: "telemetry.diagtrack", action: "drift", result: "ok", detail: "reverted by Windows" },
    { id: 6, ts: ago(95), source: "gui", tweak_id: null, action: "disk_clean", result: "ok", detail: "freed 6.2 GB" },
    { id: 5, ts: ago(180), source: "gui", tweak_id: "telemetry.policy", action: "apply", result: "ok", detail: "" },
    { id: 4, ts: ago(181), source: "gui", tweak_id: "telemetry.advertising", action: "apply", result: "ok", detail: "" },
  ];
  tweaks.push({ id: "custom.svc.adobearmservice", category: "custom", name: "Adobe Acrobat Update Service (AdobeARMservice)", description: "", risk: "moderate", warning: null,
    actions: [], custom: true, in_profile: true, drift_count: 0, status: { overall: "applied", actions: [] } });

  const replies = {
    get_state: { elevated: true, categories: cat.categories, tweaks, paused: false, startup_registered: true, backup_count: 41, data_dir: "",
      settings: { start_with_windows: true, stay_in_tray: true, watchdog_minutes: 30, dev_folders: [], idle_minutes: 120,
        auto_kill_stale: false, badge_enabled: true, badge_warn_gb: 1, badge_alert_gb: 2 } },
    get_dev_processes: dev,
    get_processes: snapshot,
    get_startup: startup,
    get_live: samples,
    get_history: history,
    get_snapshots: [],
  };
  window.__TAURI__ = {
    core: {
      invoke: async (cmd, args) => {
        if (cmd === "get_icons") return Object.fromEntries(args.paths.map((p) => [p, icon(p)]));
        return cmd in replies ? structuredClone(replies[cmd]) : null;
      },
    },
    event: { listen: async () => () => {} },
  };
})();
