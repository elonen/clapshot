#!/bin/bash

# This a Docker-side support script to run a demo of Clapshot (API server + Nginx)
# in a single Docker container for demo and testing purposes.

DIR="/mnt/clapshot-data/data"
URL_BASE=$(echo "${CLAPSHOT_URL_BASE:-http://127.0.0.1:8080/}" | sed 's#/*$#/#')
CORS="${CLAPSHOT_CORS:-$URL_BASE}"  # Default to URL_BASE

APP_TITLE="${CLAPSHOT_APP_TITLE:-Clapshot}"
LOGO_URL="${CLAPSHOT_LOGO_URL:-clapshot-logo.svg}"

# Use same URL base as index.html for API server (as Nginx proxies localhost:8095/api to /api)

WS_BASE=$(echo "$URL_BASE" | sed 's#^http#ws#')
cat > /etc/clapshot_client.conf << EOF
{
  "ws_url": "${URL_BASE}api/ws",
  "upload_url": "${URL_BASE}api/upload",
    "user_menu_extra_items": [
        { "label": "My Videos", "type": "url", "data": "/" }
    ],
  "user_menu_show_basic_auth_logout": false,
  "logo_url": "${LOGO_URL}",
  "app_title": "${APP_TITLE}"
}
EOF

# Force web user's name to 'docker' (the user we made match local user's UID here)
sed -i "s/[$]remote_user/docker/g" /etc/nginx/sites-enabled/clapshot

# Assume user accesses this at $URL_BASE
sed -i "s@^url-base.*@url-base = ${URL_BASE}@g" /etc/clapshot-server.conf

if grep -q '^cors' /etc/clapshot-server.conf; then
  sed -i "s/^cors.*/cors = '$CORS'/g" /etc/clapshot-server.conf
else
  echo "cors = '$CORS'" >> /etc/clapshot-server.conf
fi


# Make server data dir and log accessible to docker user
chown -R docker "$DIR"
touch "$DIR/clapshot.log"
chown docker "$DIR/clapshot.log"
ln -s "$DIR/clapshot.log" /var/log/

# Start nginx (in the background)
nginx

# Disable log buffering for better docker experience
export ENV PYTHONDONTWRITEBYTECODE=1
export ENV PYTHONUNBUFFERED=1

cat <<- "EOF"
==============================================
     _____ _                 _           _
    / ____| |               | |         | |
   | |    | | __ _ _ __  ___| |__   ___ | |_
   | |    | |/ _` | '_ \/ __| '_ \ / _ \| __|
   | |____| | (_| | |_) \__ | | | | (_) | |_
    \_____|_|\__,_| .__/|___|_| |_|\___/ \__|
                  | |
                  |_|
EOF
cat <<-EOF
---  Browse ${URL_BASE}  ---
==============================================
EOF

set -v

# Dig up start command from systemd script and run it as docker user instead of www-data
CMD=$(grep 'Exec' /lib/systemd/system/clapshot-server.service | sed 's/^.*=//')
sudo -u docker $CMD &

# Follow server log
tail -f /var/log/clapshot.log
