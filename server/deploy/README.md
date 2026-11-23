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
pm2 startOrReload /opt/rigsweep/ecosystem.config.js --update-env && pm2 save   # after changing env or config
```

## Secrets

Secret files live in the **project's root folder** on the developer PC, git-ignored (`*.env`, `.env.*`), and are
copied over SSH without being printed:

```sh
bash ~/.yobotn/server/push-secret.sh rigsweep-stats apps   # ./rigsweep-stats.env -> /etc/yobotn/rigsweep-stats.env (root:apps 640)
bash ~/.yobotn/server/push-secret.sh cloudflared           # ./cloudflared.env   -> /etc/yobotn/cloudflared.env (root 600)
pm2 startOrReload /opt/rigsweep/ecosystem.config.js --update-env   # on the server, so the app picks up the change
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
- **Bot Fight Mode is off** (Security → Bots), and AI crawlers aren't blocked. Bot Fight Mode challenges
  requests from datacenter IPs, which is where AI agents (ChatGPT-User, Claude-User, PerplexityBot…), search
  crawlers and agent-readiness scanners run, so with it on they got a challenge page instead of the site. WAF skip
  rules can't exempt anything from Bot Fight Mode, so leave it off and leave "Block AI bots" off; `robots.txt` allows
  everyone. The uptime workflow requests the homepage as `Claude-User` from a GitHub runner (a datacenter IP), so
  a challenge coming back shows up as a failed run.

## What agents get

The same program answers agents and API clients (tests: `tests/agents.rs`, `scripts/check-site.js`):

- **Markdown**: every page URL also answers `Accept: text/markdown` with its `index.md` (`Vary: Accept`).
  `node scripts/prerender-site.js` writes those files; CI fails when they're stale.
- **API description**: `/openapi.json` (OpenAPI 3.1), `/.well-known/api-catalog` (RFC 9727) and the
  developer page `/developers/` (`/docs` and `/api` redirect there). Change the spec with the routes:
  `tests/agents.rs` calls every operation in it.
- **Errors**: every `/v1/*` and `/admin/api/*` error is RFC 9457 `application/problem+json` with a `code` and a
  `hint`. **Rate limits**: the report endpoints send `RateLimit-Policy` / `RateLimit`, and `Retry-After` on 429.

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

**By hand over `ssh yobotn`** (fallback, e.g. when GitHub Actions is down), from Git Bash in the repo root:

```sh
bash scripts/deploy.sh          # site + server
bash scripts/deploy.sh site     # only docs/
bash scripts/deploy.sh server   # only the binary + ecosystem.config.js
```

It runs the site checks, copies `docs/` exactly (`tar` over ssh, then `rsync --delete` on the server, owned by
`apps`), runs the server tests, builds the Linux binary in Docker (start Docker Desktop first), installs it with
the pm2 config, reloads pm2 **as `apps`** (`sudo -iu apps pm2 …`; the `pm2` alias only exists in an interactive
login), and finally checks the public URLs through Cloudflare, including the Markdown and agent checks.

**From GitHub** (`.github/workflows/deploy.yml`, the normal way): live. Every push to `main` that touches `docs/`,
`server/` or the workflow runs the site checks and the server tests, builds, uploads and reloads, then pings IndexNow
with the pages that changed. **So a push to `main` is a production deploy.** Repository settings → Secrets and variables → Actions:

| Name | Kind | Value |
| --- | --- | --- |
| `DEPLOY_HOST` | secret | `159.89.102.105` |
| `DEPLOY_USER` | secret | `apps` |
| `DEPLOY_SSH_KEY` | secret | a private key made just for this (`ssh-keygen -t ed25519`); its `.pub` goes in `/home/apps/.ssh/authorized_keys` |
| `DEPLOY_KNOWN_HOSTS` | secret | output of `ssh-keyscan 159.89.102.105` |
| `DEPLOY_ENABLED` | variable | `true` |

## Runbooks

**Roll back a bad deploy.** Revert the commit on `main` and push (`git revert <sha> && git push`): `deploy.yml`
tests, builds and reloads the previous code. Faster, from a checkout of the last good commit:
`bash scripts/deploy.sh server` (or `site`). Migrations run forward only (`server/migrations/`): if the bad deploy added one,
check that the older binary still works with the new schema before rolling back, and fix forward if it doesn't.
Never edit an applied migration.

**Service down** (uptime.yml fails or `/healthz` isn't 200). On the server: `sudo -iu apps pm2 ls`, then
`sudo -iu apps pm2 logs rigsweep --lines 200` (JSON lines; `--raw | jq .` to read them). Restart with
`sudo -iu apps pm2 restart rigsweep`. If every yobotn site is down, check `systemctl status cloudflared` and
the tunnel in the Cloudflare dashboard. The site keeps serving while the database is unreachable.

**Rotate a secret.**
- *Admin password or database password*: change it (DigitalOcean → Databases → Users for the database), update
  `rigsweep-stats.env` in the project root, run `push-secret.sh rigsweep-stats apps` and
  `pm2 startOrReload` (see Secrets).
- *Deploy SSH key*: make a new key pair, put the `.pub` in `/home/apps/.ssh/authorized_keys`, replace the
  `DEPLOY_SSH_KEY` secret in GitHub, then remove the old line from `authorized_keys`.
- *Tunnel token*: rotate it in Cloudflare Zero Trust, then `push-secret.sh cloudflared` and
  `sudo systemctl restart cloudflared`.
- *Updater signing key*: can't be swapped in place (its public key is compiled into the app). Ship a release
  signed with the old key that contains the new public key, then sign with the new key. See ADR 0003.

**Restore the database.** DigitalOcean managed Postgres keeps daily backups and point-in-time recovery: in the
DigitalOcean control panel, open the database cluster → Backups → restore to a **new** cluster at the time you
need, check the data there, then point `DATABASE_URL` at it (Secrets) and reload. Never restore over the live
cluster.

## Checking it

- Site: https://rigsweep.com · Health: https://stats.rigsweep.com/healthz · Counters: https://stats.rigsweep.com/v1/totals
- Owner dashboard: https://stats.rigsweep.com/admin (ADMIN_USER / ADMIN_PASSWORD)
- On the server: `pm2 ls`, `pm2 logs rigsweep`, `systemctl status cloudflared`, `sudo ufw status`

## Testing locally

`cargo test` in `server/` downloads and runs a real PostgreSQL (postgresql_embedded), on Windows too. The first
run takes a minute or two while it downloads. `tests/site.rs` (the site, the www redirect, the health check) and
`tests/agents.rs` (Markdown negotiation, JSON errors, rate-limit headers, the OpenAPI spec against the routes) need
no database: `cargo test --test site --test agents`.
