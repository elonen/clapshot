#!/bin/bash -e
# Apply CLAPSHOT_SERVER__* env onto /etc/clapshot-server.conf, then exec the deb's
# run-with-conf.sh wrapper (the CMD). E.g. CLAPSHOT_SERVER__URL_BASE -> url-base.
CONF=/etc/clapshot-server.conf

env | grep '^CLAPSHOT_SERVER__' | while IFS='=' read -r var val; do
    key=$(printf '%s' "$var" | sed 's/^CLAPSHOT_SERVER__//' | tr 'A-Z_' 'a-z-')
    esc=$(printf '%s' "$val" | sed 's/[&/\\]/\\&/g')
    if grep -q "^#*$key" "$CONF"; then
        sed -i "s/^#*$key.*/$key = $esc/" "$CONF"
    else
        printf '%s = %s\n' "$key" "$val" >> "$CONF"
    fi
done

exec "$@"
