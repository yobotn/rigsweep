#!/usr/bin/env bash
# Deploys rigsweep.com + stats.rigsweep.com to the yobotn server by hand, over `ssh yobotn`
# (see server/deploy/README.md). Runs from Git Bash on Windows or any Linux/macOS shell.
#
#   bash scripts/deploy.sh          site + server
#   bash scripts/deploy.sh site     only docs/ (HTML, Markdown twins, openapi.json, llms.txt...)
#   bash scripts/deploy.sh server   only the rigsweep-stats binary + pm2 config
#
# Needs: ssh access as `yobotn` (NOPASSWD sudo), Node for the site checks, Docker for the Linux build.
# DEPLOY_SSH=other-host overrides the ssh target.
set -euo pipefail
cd "$(dirname "$0")/.."
SSH_HOST="${DEPLOY_SSH:-yobotn}"
WHAT="${1:-all}"
case "$WHAT" in all | site | server) ;; *) echo "usage: $0 [all|site|server]" >&2; exit 2 ;; esac

step() { printf '\n== %s\n' "$*"; }

if [ "$WHAT" != server ]; then
  step "site checks"
  node scripts/prerender-site.js --check
  node scripts/check-site.js

  step "upload docs/ -> /opt/rigsweep/site (exact copy: removed files are deleted)"
  # tar over ssh, since Git Bash has no rsync; rsync on the server makes the swap exact
  tar -C docs --exclude CNAME --exclude .nojekyll -czf - . |
    ssh "$SSH_HOST" 'set -e; t=$(mktemp -d); tar -xzf - -C "$t"
      sudo rsync -a --delete --chown=apps:apps "$t"/ /opt/rigsweep/site/
      rm -rf "$t"; echo "site: $(sudo find /opt/rigsweep/site -type f | wc -l) files"'
fi

if [ "$WHAT" != site ]; then
  step "server tests (no database needed)"
  (cd server && cargo test --locked --test site --test agents --lib)

  step "Linux build in Docker (rust:1-bookworm, same glibc as the server)"
  src="$PWD/server"
  if pwd -W >/dev/null 2>&1; then src="$(pwd -W)/server"; fi # Git Bash: Docker wants C:/... paths
  MSYS_NO_PATHCONV=1 docker run --rm -v "$src:/src" -v rigsweep-target:/target -v rigsweep-cargo:/usr/local/cargo/registry \
    -e CARGO_TARGET_DIR=/target -w /src rust:1-bookworm \
    sh -c "cargo build --release --locked && cp /target/release/rigsweep-stats /src/target/rigsweep-stats-linux"

  step "install the binary + pm2 config, reload"
  scp -q server/target/rigsweep-stats-linux "$SSH_HOST:/tmp/rigsweep-stats"
  scp -q server/deploy/ecosystem.config.js "$SSH_HOST:/tmp/rigsweep-ecosystem.config.js"
  # pm2 runs as the apps user: the interactive `pm2` alias doesn't exist in a non-interactive ssh shell
  ssh "$SSH_HOST" 'set -e
    sudo install -o apps -g apps -m 755 /tmp/rigsweep-stats /opt/rigsweep/bin/rigsweep-stats
    sudo install -o apps -g apps -m 644 /tmp/rigsweep-ecosystem.config.js /opt/rigsweep/ecosystem.config.js
    rm -f /tmp/rigsweep-stats /tmp/rigsweep-ecosystem.config.js
    sudo -iu apps pm2 startOrReload /opt/rigsweep/ecosystem.config.js --update-env >/dev/null
    sudo -iu apps pm2 save >/dev/null
    sleep 2; curl -fsS http://127.0.0.1:2000/healthz; echo " (local health)"'
fi

step "public checks through Cloudflare"
fail=0
check() { # url, expected status, expected content-type prefix, extra curl args...
  local url=$1 want=$2 type=$3; shift 3
  local got
  got=$(curl -s -o /dev/null -m 20 -w '%{http_code} %{content_type}' "$@" "$url") || true
  if [ "${got%% *}" = "$want" ] && [[ "${got#* }" == "$type"* ]]; then echo "ok   $url $*"; else echo "FAIL $url $*: got '$got', want $want $type"; fail=1; fi
}
check https://rigsweep.com/ 200 text/html
check https://rigsweep.com/ 200 text/markdown -H "Accept: text/markdown"
check https://rigsweep.com/ 200 text/html -A "Claude-User"
check https://rigsweep.com/developers/ 200 text/html
check https://rigsweep.com/openapi.json 200 application/json
check https://rigsweep.com/.well-known/api-catalog 200 application/linkset+json
check https://rigsweep.com/llms.txt 200 text/plain
check https://rigsweep.com/sitemap.xml 200 text/xml
check https://stats.rigsweep.com/healthz 200 text/plain
check https://stats.rigsweep.com/v1/totals 200 application/json
check https://stats.rigsweep.com/v1/nope 404 application/problem+json
exit $fail
