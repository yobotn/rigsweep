// pm2 config for RigSweep on the yobotn server: /opt/rigsweep/ecosystem.config.js
// Start or reload:  pm2 startOrReload /opt/rigsweep/ecosystem.config.js && pm2 save
// Secrets come from /etc/yobotn/rigsweep-stats.env (root:apps 640), never from this file.
const fs = require("fs");

function envFile(path) {
  const out = {};
  for (const line of fs.readFileSync(path, "utf8").split(/\r?\n/)) {
    const m = line.match(/^\s*([A-Z0-9_]+)\s*=\s*(.*?)\s*$/);
    if (m) out[m[1]] = m[2];
  }
  return out;
}

module.exports = {
  apps: [
    {
      name: "rigsweep",
      script: "/opt/rigsweep/bin/rigsweep-stats",
      interpreter: "none",
      cwd: "/opt/rigsweep",
      env: {
        ...envFile("/etc/yobotn/rigsweep-stats.env"),
        BIND: "127.0.0.1:2000", // rigsweep.com, www and stats all route here through the tunnel
        SITE_DIR: "/opt/rigsweep/site",
        LOG_FORMAT: "json", // one JSON object per log line: pm2 logs rigsweep --raw | jq
      },
      autorestart: true,
      restart_delay: 3000,
      max_memory_restart: "256M",
      kill_timeout: 5000,
    },
  ],
};
