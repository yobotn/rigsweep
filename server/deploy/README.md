# Server setup: rigsweep.com + stats.rigsweep.com

One small Ubuntu server runs Caddy (HTTPS and the static site) and the stats collector (a single
binary run by systemd). Data lives in the DigitalOcean managed Postgres cluster. No Docker.

```
browser / app ──HTTPS──> Caddy ──┬── rigsweep.com        → /var/www/rigsweep (docs/)
                                 └── stats.rigsweep.com  → 127.0.0.1:8787 (rigsweep-stats)
                                                              │ TLS, private network
                                                              └──> managed Postgres: database "rigsweep"
```

Downloads and app updates stay on GitHub Releases. The updater keeps working even if this server is down.

## 1. Server

- DigitalOcean server: Ubuntu 24.04, the smallest size (1 GB RAM is plenty), **in the same region
  and VPC as the Postgres cluster**.
- Firewall: allow 22, 80 and 443 only.

```sh
# as root
adduser --disabled-password --gecos "" deploy
mkdir -p /home/deploy/.ssh && cp ~/.ssh/authorized_keys /home/deploy/.ssh/   # or add the deploy key below
chown -R deploy:deploy /home/deploy/.ssh
install -d -o deploy -g deploy /opt/rigsweep-stats /var/www/rigsweep
echo 'deploy ALL=(root) NOPASSWD: /usr/bin/systemctl restart rigsweep-stats' > /etc/sudoers.d/deploy
chmod 440 /etc/sudoers.d/deploy

# Caddy (official repository)
apt install -y debian-keyring debian-archive-keyring apt-transport-https curl
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' > /etc/apt/sources.list.d/caddy-stable.list
apt update && apt install -y caddy
```

## 2. Database (in the managed cluster)

In the DigitalOcean console, under the cluster:
1. **Users & Databases**: add the database `rigsweep` and the user `rigsweep_stats`.
2. **Trusted sources**: add this server.
3. Connect as `doadmin` and give the new user its own database, so it can't touch other projects:

```sql
ALTER DATABASE rigsweep OWNER TO rigsweep_stats;
\c rigsweep
ALTER SCHEMA public OWNER TO rigsweep_stats;
```

4. Download the cluster's **CA certificate** to `/etc/rigsweep-ca.crt` on the server (`chmod 644`).

The collector creates its tables itself on first start (`migrations/`).

## 3. Collector and Caddy

```sh
cp rigsweep-stats.service /etc/systemd/system/
cp rigsweep-stats.env.example /etc/rigsweep-stats.env && chmod 600 /etc/rigsweep-stats.env
nano /etc/rigsweep-stats.env          # DATABASE_URL (private host) and ADMIN_PASSWORD
cp update-geoip.sh /usr/local/bin/ && chmod 755 /usr/local/bin/update-geoip.sh && /usr/local/bin/update-geoip.sh
echo '17 4 3 * * root /usr/local/bin/update-geoip.sh' > /etc/cron.d/rigsweep-geoip
cp Caddyfile /etc/caddy/Caddyfile
systemctl daemon-reload && systemctl enable rigsweep-stats
systemctl reload caddy
```

The service starts working after the first deploy uploads the binary.

## 4. GitHub deploys

On the repository, under Settings → Secrets and variables → Actions:

| Name | Kind | Value |
| --- | --- | --- |
| `DEPLOY_HOST` | secret | the server's IP |
| `DEPLOY_USER` | secret | `deploy` |
| `DEPLOY_SSH_KEY` | secret | a private key made just for this (`ssh-keygen -t ed25519`); its `.pub` goes in `/home/deploy/.ssh/authorized_keys` |
| `DEPLOY_KNOWN_HOSTS` | secret | output of `ssh-keyscan <server ip>` |
| `DEPLOY_ENABLED` | variable | `true` |

Run **Actions → deploy → Run workflow** once. Later, every push to `main` that touches `docs/` or `server/` deploys.

## 5. Move the domain off GitHub Pages

1. Lower the DNS TTL to 300 a day ahead.
2. Point the `A` records for `rigsweep.com`, `www` and `stats` to the server. Caddy gets the certificates within a minute.
3. Check https://rigsweep.com and https://stats.rigsweep.com/healthz.
4. Turn off GitHub Pages (Settings → Pages) and delete `docs/CNAME`.

## Checking it

- Public counters: `https://stats.rigsweep.com/v1/totals`
- Owner dashboard: `https://stats.rigsweep.com/admin` (ADMIN_USER / ADMIN_PASSWORD)
- Logs: `journalctl -u rigsweep-stats -f` (errors only; client IPs are never logged)

## Testing locally

`cargo test` in `server/` downloads and runs a real PostgreSQL (postgresql_embedded), on Windows
too. The first run takes a minute or two while it downloads.
