#!/bin/sh
# Monthly: fetch DB-IP's free "IP to Country Lite" database (CC BY 4.0, attribution on the
# privacy page) and restart the collector so it picks the new file up.
# Install: cp update-geoip.sh /usr/local/bin/ && chmod 755 /usr/local/bin/update-geoip.sh
# Cron (root): 17 4 3 * * /usr/local/bin/update-geoip.sh
set -eu
dir=/var/lib/rigsweep-geo
mkdir -p "$dir"
month=$(date -u +%Y-%m)
tmp=$(mktemp)
if ! curl -fsSL "https://download.db-ip.com/free/dbip-country-lite-$month.mmdb.gz" -o "$tmp"; then
	# early in the month the new file may not be out yet: keep last month's
	month=$(date -u -d "$(date -u +%Y-%m-01) -1 day" +%Y-%m)
	curl -fsSL "https://download.db-ip.com/free/dbip-country-lite-$month.mmdb.gz" -o "$tmp"
fi
gunzip -c "$tmp" > "$dir/country.mmdb.new"
rm -f "$tmp"
chmod 644 "$dir/country.mmdb.new"
mv -f "$dir/country.mmdb.new" "$dir/country.mmdb"
systemctl try-restart rigsweep-stats
