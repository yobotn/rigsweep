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
    row({ pid: 21740, label: "WSL · Ubuntu", name: "wsl.exe", eco: "wsl", project: "npm run watch", tree_count: 5, age_secs: 6.3 * H, tree_mem: 41 * MB, reasons: ["orphan"], stale: true }),
    row({ pid: 16620, label: "Git Bash", name: "tail.exe", eco: "shell", cmd: "tail -f server.log · grep -v DEBUG · sleep 30", tree_count: 7, age_secs: 11.5 * H, tree_mem: 38 * MB, reasons: ["orphan"], stale: true }),
    row({ pid: 30112, label: "WSL 2 / Docker", name: "vmmemWSL", kind: "vm", eco: "wsl", project: "docker-desktop, Ubuntu", age_secs: 0, tree_mem: 2.1 * GB }),
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
    // the demo PC has no Dell / ASUS / HP / Lenovo software, like most custom-built dev machines
    if (x.category === "vendor") return { ...x, in_profile: false, drift_count: 0,
      status: { overall: "not_present", actions: x.actions.map((a) => ({ target: actionTarget(a), status: { state: "not_present" } })) } };
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

  const F = (area, kind, bytes, count = 1) => ({ area, kind, bytes, count });
  const kinds = (k) => k.map((x) => F(...x));
  const period = (disk, memory, browser, processes, cleanups, k) => ({ disk, memory, browser, processes, cleanups, kinds: kinds(k) });
  const impact = {
    week: period(14.2 * GB, 6.8 * GB, 1.1 * GB, 23, 9, [["disk", "dir:target", 9.6 * GB], ["memory", "mcp", 3.1 * GB, 11], ["disk", "dir:node_modules", 2.9 * GB, 4], ["memory", "browser", 2.2 * GB, 5], ["disk", "cache:npm", 1.7 * GB], ["browser", "browser:cache", 1.1 * GB]]),
    month: period(61.5 * GB, 24.3 * GB, 3.9 * GB, 87, 31, [["disk", "dir:target", 38.2 * GB, 7], ["disk", "dir:node_modules", 11.4 * GB, 12], ["memory", "mcp", 10.6 * GB, 40], ["memory", "browser", 7.9 * GB, 19], ["disk", "cache:pnpm", 6.3 * GB], ["browser", "browser:cache", 3.9 * GB]]),
    all: period(212 * GB, 71 * GB, 12 * GB, 304, 118, [["disk", "dir:target", 121 * GB, 25], ["disk", "dir:node_modules", 41 * GB, 50], ["memory", "mcp", 30 * GB, 130], ["disk", "cache:cargo", 22 * GB], ["memory", "browser", 21 * GB, 60]]),
    since: "2026-09-02 10:14:00",
  };
  // ?notice=opt_out / opt_in shows the first-run stats notice; otherwise it counts as already seen
  const notice = q.get("notice");
  const share = { enabled: notice !== "opt_in", chosen: false, notice_seen: !notice, consent_default: notice || "opt_out",
    install_id: "3f9c2a71d04e4b8c9a6e15b27d8c40f1", last_sent: notice ? null : ago(2 * 24 * 60), last_report: null };
  const report = { v: 1, install_id: share.install_id, app_version: "0.8.0", windows_build: "26100", processes: 23, cleanups: 9,
    freed: impact.week.kinds, machine: { ram_total: 32 * GB, ram_used_avg: 21.4 * GB, cores: 16 },
    top_cpu: [{ app: "Chrome", cpu: 38.5, mem: 4.1 * GB }, { app: "VS Code", cpu: 12.2, mem: 2.3 * GB }, { app: "Node.js", cpu: 9.8, mem: 1.9 * GB }],
    top_mem: [{ app: "Chrome", cpu: 38.5, mem: 4.1 * GB }, { app: "WSL / Docker VM", cpu: 3.1, mem: 3.6 * GB }, { app: "VS Code", cpu: 12.2, mem: 2.3 * GB }],
    tweaks: ["telemetry.advertising", "telemetry.diagtrack", "telemetry.policy"] };
  // Dev Disk and browser caches (demo values; the Dashboard's "Ready to free" reads them too)
  const cache = (id, eco, tier, bytes) => ({ id, eco, tier, bytes, total: bytes, disabled: false, busy: false, tool: "", keep_newest: false, show_only: false, paths: [] });
  const art = (name, eco, tier, bytes) => ({ name, eco, tier, bytes, pnpm: false, suggest: true });
  const proj = (path, idle, items) => ({ path, last_active: 0, idle_days: idle, busy: false, onedrive: false, stale: idle >= 30, items });
  const projects = [
    proj("C:\\Users\\dev\\code\\android-app", 64, [art("build", "java", "build", 3.8 * GB), art(".gradle", "java", "build", 1.2 * GB)]),
    proj("C:\\Users\\dev\\code\\rust-cli", 41, [art("target", "rust", "build", 9.6 * GB)]),
    proj("C:\\Users\\dev\\code\\old-landing", 120, [art("node_modules", "node", "deps", 612 * 1048576), art(".next", "node", "build", 284 * 1048576)]),
    proj("C:\\Users\\dev\\code\\shop-web", 2, [art("node_modules", "node", "deps", 890 * 1048576)]),
  ];
  const caches = [cache("npm", "node", "safe", 2.4 * GB), cache("pnpm", "node", "safe", 3.1 * GB), cache("cargo", "rust", "safe", 1.7 * GB), cache("pip", "python", "safe", 640 * 1048576), { ...cache("nugetpkgs", "dotnet", "safe", 1.1 * GB), disabled: true }];
  const diskReport = { caches, projects, worktrees: [], roots: ["C:\\Users\\dev\\code"], home: "C:\\Users\\dev",
    cache_total: caches.reduce((a, c) => a + c.bytes, 0), project_total: projects.reduce((a, x) => a + x.items.reduce((b, i) => b + i.bytes, 0), 0),
    stale_total: 0, worktree_total: 0, scan_ms: 4200 };
  const bitem = (key, tier, bytes) => ({ key, tier, bytes });
  const browsers = { safe_total: 1.9 * GB, site_total: 420 * 1048576, browsers: [
    { id: "chrome", name: "Chrome", running: false, profiles: [{ dir: "Default", name: "Personal", items: [bitem("cache", "safe", 1.1 * GB), bitem("gpu", "safe", 96 * 1048576), bitem("indexeddb", "site", 420 * 1048576)] }] },
    { id: "edge", name: "Edge", running: false, profiles: [{ dir: "Default", name: "Work", items: [bitem("cache", "safe", 640 * 1048576), bitem("sw", "safe", 88 * 1048576)] }] },
  ] };
  // screenshots never show the "freed 100 GB" milestone toast
  try { localStorage.setItem("impact_ms", "1000"); } catch {}
  const replies = {
    get_impact: impact,
    get_share_state: share,
    get_report_preview: { report, consent_default: share.consent_default },
    get_state: { elevated: true, categories: cat.categories, tweaks, paused: false, startup_registered: true, backup_count: 41, data_dir: "",
      settings: { start_with_windows: true, stay_in_tray: true, watchdog_minutes: 30, dev_folders: [], idle_minutes: 120,
        auto_kill_stale: false, badge_enabled: true, badge_warn_gb: 1, badge_alert_gb: 2, disk_excludes: ["C:\\Users\\dev\\code\\client-archive"] } },
    get_dev_processes: dev,
    get_processes: snapshot,
    get_startup: startup,
    get_live: samples,
    get_history: history,
    get_snapshots: [],
    get_dev_disk: diskReport,
    process_why: { pid: 18244, name: "node.exe", exe: "C:\\Program Files\\nodejs\\node.exe", cmd: "node C:\\Users\\dev\\code\\shop-web\\node_modules\\vite\\bin\\vite.js --port 5173",
      start_time: Math.floor(Date.now() / 1000) - 33000, cpu: 7.2, mem: 612 * 1048576, disk_read: 1.2 * 1048576, disk_write: 300 * 1024, threads: 14,
      parents: [{ pid: 15032, name: "pwsh.exe", cpu: 0, mem: 80 * 1048576 }, { pid: 9876, name: "Code.exe", cpu: 9.7, mem: 1.2 * GB }],
      children: [{ pid: 18301, name: "esbuild.exe", cpu: 2.1, mem: 96 * 1048576 }], company: "Node.js", description: "Node.js JavaScript Runtime" },
    get_gpu: { npu: false, adapters: [
      { luid: "0x0_0x1", name: "NVIDIA GeForce RTX 4070 Laptop GPU", util: 63, npu: false, dedicated: 9.4 * GB, shared: 410 * 1048576,
        engines: [{ kind: "3D", util: 12 }, { kind: "Compute", util: 61 }, { kind: "Copy", util: 3 }, { kind: "VideoDecode", util: 0 }] },
      { luid: "0x0_0x2", name: "Intel(R) Arc(TM) Graphics", util: 4, npu: false, dedicated: 0, shared: 1.1 * GB, engines: [{ kind: "3D", util: 4 }] }],
      procs: [
        { pid: 14220, name: "ollama_llama_server.exe", util: 58, engine: "Compute", adapter: "0x0_0x1", npu_util: 0, dedicated: 7.9 * GB, shared: 120 * 1048576 },
        { pid: 20410, name: "python.exe", util: 3, engine: "Compute", adapter: "0x0_0x1", npu_util: 0, dedicated: 1.2 * GB, shared: 64 * 1048576 },
        { pid: 9876, name: "Code.exe", util: 2.1, engine: "3D", adapter: "0x0_0x2", npu_util: 0, dedicated: 0, shared: 210 * 1048576 },
        { pid: 3120, name: "dwm.exe", util: 4.4, engine: "3D", adapter: "0x0_0x2", npu_util: 0, dedicated: 0, shared: 380 * 1048576 }] },
    get_ai_workload: [
      { pid: 13990, name: "ollama.exe", label: "Ollama", models: [{ name: "qwen3-coder:14b", vram: 7.8 * GB, size: 9.1 * GB }], vram: 7.9 * GB, util: 58, npu_util: 0 },
      { pid: 20410, name: "python.exe", label: "ComfyUI (Python)", models: [], vram: 1.2 * GB, util: 3, npu_util: 0 }],
    get_net_totals: { in_per_sec: 2.4 * 1048576, out_per_sec: 310 * 1024, ifaces: [
      { name: "Wi-Fi", description: "Intel(R) Wi-Fi 7 BE200 320MHz", kind: "wifi", hardware: true, link_speed: 2.4e9, in_per_sec: 2.4 * 1048576, out_per_sec: 310 * 1024, in_total: 18.2 * GB, out_total: 2.1 * GB }] },
    get_connections: { conns: [], procs: [
      { pid: 18244, name: "node.exe", listening: [5173], udp: [], established: 2, tcp: 3, remotes: [{ host: "127.0.0.1", count: 2 }] },
      { pid: 9876, name: "Code.exe", listening: [], udp: [], established: 14, tcp: 14, remotes: [{ host: "140.82.112.21", count: 6 }, { host: "13.107.42.18", count: 4 }] },
      { pid: 13990, name: "ollama.exe", listening: [11434], udp: [], established: 0, tcp: 1, remotes: [] },
      { pid: 19876, name: "python.exe", listening: [8000], udp: [], established: 1, tcp: 2, remotes: [] }] },
    get_port_owner: [{ pid: 11408, name: "node.exe", cmd: "node C:\\Users\\dev\\code\\dashboard\\node_modules\\next\\dist\\bin\\next dev", proto: "tcp", states: ["LISTEN"] }],
    finders_orphan_programs: [
      { kind: "dead_entry", name: "OldVPN Client 3.2", publisher: "OldVPN Inc.", version: "3.2", reg_key: "HKLM\\SOFTWARE\\...\\OldVPN", missing_file: "C:\\Program Files\\OldVPN\\uninst.exe", estimated_kb: 0, path: "", bytes: 0, modified: 0, has_exe: false },
      { kind: "leftover_folder", name: "PhotoTool", publisher: "", reg_key: "", missing_file: "", path: "C:\\Program Files\\PhotoTool", bytes: 412 * 1048576, modified: 1690000000, has_exe: false },
      { kind: "leftover_folder", name: "IP Scanner Portable", publisher: "", reg_key: "", missing_file: "", path: "C:\\Program Files (x86)\\IP Scanner Portable", bytes: 38 * 1048576, modified: 1700000000, has_exe: true }],
    finders_old_files: { folders: [], truncated: false, total_found: 3, total_bytes: 6.9 * GB, files: [
      { path: "C:\\Users\\dev\\Downloads\\ubuntu-22.04-desktop-amd64.iso", bytes: 4.7 * GB, modified: 1672000000, accessed: 0, category: "big_old", onedrive: false },
      { path: "C:\\Users\\dev\\Documents\\old-recordings\\meeting.mp4", bytes: 1.9 * GB, modified: 1660000000, accessed: 0, category: "big_old", onedrive: true },
      { path: "C:\\Users\\dev\\Downloads\\SomeApp-Setup-1.2.exe", bytes: 310 * 1048576, modified: 1720000000, accessed: 0, category: "old_installer", onedrive: false }] },
    finders_worth_a_look: [
      { path: "C:\\Users\\dev\\AppData\\Roaming\\updater\\upd.exe", reason: "unsigned_autostart", signature: "unsigned", signer: "", autostart: "HKCU\\...\\Run : Updater", running: true, sha256: "9f2c4e1a7b3d5e6f8a9b0c1d2e3f4a5b", bytes: 1.2 * 1048576, modified: 1730000000 },
      { path: "C:\\Users\\dev\\Downloads\\invoice.pdf.exe", reason: "double_extension", signature: "unsigned", signer: "", autostart: "", running: false, sha256: "", bytes: 640 * 1024, modified: 1731000000 }],
    finders_defender_scan: { verdict: "clean", detail: "", exit_code: 0 },
    get_browser_caches: browsers,
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
