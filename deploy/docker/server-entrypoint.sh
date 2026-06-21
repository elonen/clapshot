#!/bin/bash -e
# Apply CLAPSHOT_SERVER__* env onto /etc/clapshot-server.conf, then exec the deb's
# run-with-conf.sh wrapper (the CMD). E.g. CLAPSHOT_SERVER__URL_BASE -> url-base.
# Edit a /tmp copy and write it back: the container runs as www-data, which can't create
# sed -i's temp file in /etc (only the conf file itself is www-data-owned, not the dir).
CONF=/etc/clapshot-server.conf
tmp=$(mktemp)
cp "$CONF" "$tmp"

env | grep '^CLAPSHOT_SERVER__' | while IFS='=' read -r var val; do
    key=$(printf '%s' "$var" | sed 's/^CLAPSHOT_SERVER__//' | tr 'A-Z_' 'a-z-')
    esc=$(printf '%s' "$val" | sed 's/[&/\\]/\\&/g')
    if grep -q "^#*$key" "$tmp"; then
        sed -i "s/^#*$key.*/$key = $esc/" "$tmp"
    else
        printf '%s = %s\n' "$key" "$val" >> "$tmp"
    fi
done

cat "$tmp" > "$CONF"      # CONF is www-data-owned, so truncate+write works
rm -f "$tmp"

exec "$@"
