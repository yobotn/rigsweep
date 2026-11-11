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

  const replies = {
    get_state: { elevated: true, categories: [], tweaks: [], paused: false, startup_registered: true, backup_count: 0, data_dir: "",
      settings: { start_with_windows: true, stay_in_tray: true, watchdog_minutes: 30, dev_folders: [], idle_minutes: 120,
        auto_kill_stale: false, badge_enabled: true, badge_warn_gb: 1, badge_alert_gb: 2 } },
    get_dev_processes: dev,
    get_live: samples,
    get_history: [],
    get_snapshots: [],
  };
  window.__TAURI__ = {
    core: { invoke: async (cmd) => (cmd in replies ? structuredClone(replies[cmd]) : null) },
    event: { listen: async () => () => {} },
  };
})();
