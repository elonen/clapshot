#!/bin/bash

# This a Docker-side support script to run a demo of Clapshot (API server + Nginx)
# in a single Docker container for demo and testing purposes.

DIR="/mnt/clapshot-data/data"
URL_BASE="http://127.0.0.1:8080/"

# Serve client with Nginx
rm -f /etc/nginx/sites-enabled/*
cp /usr/share/doc/clapshot-client/examples/clapshot.nginx.conf  /etc/nginx/sites-enabled/clapshot

# Use same URL base as index.html for API server (as Nginx proxies localhost:8095/api to /api)
echo '{"api_url": "//"}' > /etc/clapshot_client.conf

# Force web user's name to 'docker' (the user we made matche local user's UID here)
sed -i "s/[$]remote_user/docker/g" /etc/nginx/sites-enabled/clapshot

# Assume user accesses this at "http://127.0.0.1:8080/"
sed -i "s@^url-base.*@url-base = ${URL_BASE}/@g" /etc/clapshot-server.conf


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

set -v

echo <<- "EOF"
==============================================
  _____                   _
 |  __ \                 (_)
 | |__) _   _ _ __  _ __  _ _ __   __ _
 |  _  | | | | '_ \| '_ \| | '_ \ / _` |
 | | \ | |_| | | | | | | | | | | | (_| |
 |_|  \_\__,_|_| |_|_| |_|_|_| |_|\__, |
     _____ _                 _     __/ | _
    / ____| |               | |   |___/ | |
   | |    | | __ _ _ __  ___| |__   ___ | |_
   | |    | |/ _` | '_ \/ __| '_ \ / _ \| __|
   | |____| | (_| | |_) \__ | | | | (_) | |_
    \_____|_|\__,_| .__/|___|_| |_|\___/ \__|
                  | |
                  |_|

---  Browse http://127.0.0.1:8080 then    ---
---  copy some videos to ./test/VOLUME/    ---
==============================================
EOF

# Dig up start command from systemd script and run it as docker user instead of www-data
CMD=$(grep 'Exec' /lib/systemd/system/clapshot-server.service | sed 's/^.*=//')
sudo -u docker $CMD &

# Follow server log
tail -f /var/log/clapshot.log
