# Server setup: rigsweep.com + stats.rigsweep.com

RigSweep runs on the **yobotn** server (DigitalOcean, Ubuntu 24.04), which also hosts the other yobotn apps.
One program, `rigsweep-stats` (Rust: axum + sqlx), serves both the static site (`docs/`) and the stats API.
pm2 keeps it running. Cloudflare sits in front of everything. No web server, no Docker (yet).

```
visitor ─HTTPS─> Cloudflare (DNS, SSL, WAF, DDoS)
                   │  Cloudflare Tunnel: outbound from the server, no open web ports
                   ▼
           cloudflared (systemd) ──> 127.0.0.1:2000  rigsweep-stats (pm2, user "apps")
                                        ├── rigsweep.com, www (301), static files from /opt/rigsweep/site
                                        └── stats.rigsweep.com: /v1/*, /admin, /healthz
                                               │ TLS over the private network (VPC)
                                               └──> DigitalOcean managed Postgres: database "rigsweep"
```

Downloads and app updates stay on GitHub Releases. The updater keeps working even if this server is down.

## Ports

Each app gets its own round port on `127.0.0.1` (never public). Ports under 1024 need root, so they start at 2000.

| Port | App | Hostnames (tunnel routes) |
| --- | --- | --- |
| 2000 | RigSweep (site + stats) | `rigsweep.com`, `www.rigsweep.com`, `stats.rigsweep.com` |
| 3000, 4000, … | next apps | |

## What's on the server

| Piece | Where | Notes |
| --- | --- | --- |
| SSH | port 22, key only | admin user `yobotn` (sudo). Root login and passwords are off. Emergency: the DigitalOcean web console. |
| Firewall | ufw | only 22 is open; fail2ban on SSH; automatic security updates |
| cloudflared | systemd `cloudflared.service` | token in `/etc/yobotn/cloudflared.env` (root, 600); routes are managed in the Cloudflare dashboard |
| Apps | pm2 as user `apps`, started at boot by `pm2-apps.service` | logs in `/home/apps/.pm2/logs`, rotated (10 MB × 14) |
| RigSweep | `/opt/rigsweep/{bin,site,ecosystem.config.js}` | owned by `apps` |
| Secrets | `/etc/yobotn/<name>.env` | app files are `root:apps 640`; never in git |

Working with pm2 (as `yobotn`, the alias runs it as `apps`):

```sh
pm2 ls                        # every app, status, memory, restarts
pm2 logs rigsweep             # live logs (errors only; client IPs are never logged)
pm2 reload rigsweep           # restart without dropping requests
pm2 startOrReload /opt/rigsweep/ecosystem.config.js && pm2 save   # after changing env or config
```

## Secrets

Secret files live in the **project's root folder** on the developer PC, git-ignored (`*.env`, `.env.*`), and are
copied over SSH without being printed:

```sh
bash ~/.yobotn/server/push-secret.sh rigsweep-stats apps   # ./rigsweep-stats.env -> /etc/yobotn/rigsweep-stats.env (root:apps 640)
bash ~/.yobotn/server/push-secret.sh cloudflared           # ./cloudflared.env   -> /etc/yobotn/cloudflared.env (root 600)
pm2 startOrReload /opt/rigsweep/ecosystem.config.js        # on the server, so the app picks up the change
```

`rigsweep-stats.env` (see `rigsweep-stats.env.example`): `DATABASE_URL`, `ADMIN_USER`, `ADMIN_PASSWORD`,
`TRUST_CF_COUNTRY=1`, `RATE_MAX`, `RUST_LOG`. `BIND` and `SITE_DIR` are set in `ecosystem.config.js`.

## Database (DigitalOcean managed Postgres)

1. **Users & Databases**: add the database `rigsweep` and the user `rigsweep_stats`.
2. **Trusted sources**: add the yobotn droplet.
3. Connect as `doadmin` and give the new user its own database, so it can't touch other projects:

   ```sql
   ALTER DATABASE rigsweep OWNER TO rigsweep_stats;
   \c rigsweep
   ALTER SCHEMA public OWNER TO rigsweep_stats;
   ```

4. Use the **VPC (private) host**, port 25060, in `DATABASE_URL`, ending with
   `?sslmode=verify-full&sslrootcert=/etc/yobotn/rigsweep-ca.crt`. Put the cluster's CA certificate at that path
   (`chmod 644`).

The collector creates its tables itself (`migrations/`). If the database is unreachable, the site keeps working and
the migrations are retried every 15 seconds.

## Cloudflare

- DNS for the domain is on Cloudflare. **Don't add A records for the server**: each tunnel route creates its own
  proxied DNS record, and the server's IP stays out of DNS.
- Tunnel `yobotn` → **Routes → Add route → Published application**: hostname → `http://localhost:<port>`.
- The country comes from Cloudflare's `CF-IPCountry` header (Network → IP Geolocation must be on). No GeoIP database.
- The visitor IP (rate limit only, never stored) comes from `CF-Connecting-IP`. Only the tunnel can reach the app.
- Recommended: SSL/TLS → **Always Use HTTPS**; Security → WAF managed rules.

## First-time server setup

Done once on 2026-11-17 with `~/.yobotn/server/harden.sh`: hostname, updates, the `yobotn` admin user, key-only
SSH, ufw, fail2ban, swap, `/etc/yobotn`. Then:

```sh
# cloudflared (Cloudflare's apt repo) + a systemd unit that reads TUNNEL_TOKEN from /etc/yobotn/cloudflared.env
# Node 24 (NodeSource) + pm2, and the apps user:
sudo npm install -g pm2
sudo adduser --disabled-password --gecos "" apps
sudo chgrp apps /etc/yobotn && sudo chmod 750 /etc/yobotn
sudo install -d -o apps -g apps /opt/rigsweep /opt/rigsweep/bin /opt/rigsweep/site
sudo env PATH=$PATH:/usr/bin pm2 startup systemd -u apps --hp /home/apps
sudo -iu apps pm2 install pm2-logrotate
```

## Deploys

**From GitHub** (`.github/workflows/deploy.yml`): every push to `main` that touches `docs/` or `server/` tests,
builds, uploads and reloads. Repository settings → Secrets and variables → Actions:

| Name | Kind | Value |
| --- | --- | --- |
| `DEPLOY_HOST` | secret | `159.89.102.105` |
| `DEPLOY_USER` | secret | `apps` |
| `DEPLOY_SSH_KEY` | secret | a private key made just for this (`ssh-keygen -t ed25519`); its `.pub` goes in `/home/apps/.ssh/authorized_keys` |
| `DEPLOY_KNOWN_HOSTS` | secret | output of `ssh-keyscan 159.89.102.105` |
| `DEPLOY_ENABLED` | variable | `true` |

**By hand** from the PC (Linux build in Docker, so it matches the server):

```sh
docker run --rm -v "$PWD/server:/src" -v rigsweep-target:/target -v rigsweep-cargo:/usr/local/cargo/registry \
  -e CARGO_TARGET_DIR=/target -w /src rust:1-bookworm \
  sh -c "cargo build --release --locked && cp /target/release/rigsweep-stats /src/target/rigsweep-stats-linux"
scp server/target/rigsweep-stats-linux yobotn:/tmp/rigsweep-stats
ssh yobotn 'sudo install -o apps -g apps -m 755 /tmp/rigsweep-stats /opt/rigsweep/bin/rigsweep-stats && pm2 reload rigsweep'
```

## Checking it

- Site: https://rigsweep.com · Health: https://stats.rigsweep.com/healthz · Counters: https://stats.rigsweep.com/v1/totals
- Owner dashboard: https://stats.rigsweep.com/admin (ADMIN_USER / ADMIN_PASSWORD)
- On the server: `pm2 ls`, `pm2 logs rigsweep`, `systemctl status cloudflared`, `sudo ufw status`

## Testing locally

`cargo test` in `server/` downloads and runs a real PostgreSQL (postgresql_embedded), on Windows too. The first
run takes a minute or two while it downloads. `tests/site.rs` covers the site, the www redirect and the health check
without a database.
