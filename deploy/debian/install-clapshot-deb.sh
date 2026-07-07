#!/bin/bash
set -e

# This script installs Clapshot with HTWicket (login form + user management)
# on Debian 12 Bookworm or Debian 13 Trixie
#
# It doesn't set up HTTPS, you are encouraged to
# set use a reverse proxy in front of it.
#
# First, mount a block device at $DATA_DIR run `apt update`.
# Then run this script.
#
# It also displays the configuration files after modifying them,
# so you can check that they look sensible.
# ------------------------

# Default values
DATA_DIR="/mnt/clapshot-data"

# Parse command line arguments
usage() {
    echo "Usage: $0 -a PUBLIC_ADDRESS [-d DATA_DIR]"
    echo ""
    echo "Options:"
    echo "  -a PUBLIC_ADDRESS  The address clients will use to access this host."
    echo "                     Example: http://clapshot.example.com"
    echo "                     Use https:// if you have a separate HTTPS reverse proxy."
    echo ""
    echo "  -d DATA_DIR        Directory to store videos and the database."
    echo "                     Default: $DATA_DIR"
    echo ""
    exit 1
}
while getopts "a:d:" opt; do
    case "$opt" in
        a) PUBLIC_ADDRESS=$OPTARG ;;
        d) DATA_DIR=$OPTARG ;;
        *) usage ;;
    esac
done
if [ -z "$PUBLIC_ADDRESS" ]; then
    echo "** Error: PUBLIC_ADDRESS is required."
    echo " "
    usage
fi
case "$PUBLIC_ADDRESS" in
    http://*|https://*) ;;
    *) echo "** Error: PUBLIC_ADDRESS must include a scheme, e.g. http://clapshot.example.com (got: '$PUBLIC_ADDRESS')"; echo " "; usage ;;
esac

set -x

RELEASE="0.12.1"

# Autodetect Debian release (codename) and architecture from the running system
. /etc/os-release
DEBIAN_VER="$VERSION_CODENAME"
case "$DEBIAN_VER" in
    bookworm) PYTHON_VER="3.11" ;;
    trixie)   PYTHON_VER="3.13" ;;
    *) echo "Unsupported Debian release: '${DEBIAN_VER}' (supported: bookworm, trixie)"; exit 1 ;;
esac

case "$(uname -m)" in
    x86_64)  DEB_ARCH="amd64" ;;
    aarch64) DEB_ARCH="arm64" ;;
    *) echo "Unsupported architecture: $(uname -m)"; exit 1 ;;
esac

BASE="https://github.com/elonen/clapshot/releases/download/v${RELEASE}"
CLIENT_LINK="${BASE}/clapshot-client_${RELEASE}_${DEBIAN_VER}_all.deb"
SERVER_LINK="${BASE}/clapshot-server_${RELEASE}-1_${DEBIAN_VER}_${DEB_ARCH}.deb"
ORG_LINK="${BASE}/clapshot-organizer-basic-folders_${RELEASE}_${DEBIAN_VER}_${DEB_ARCH}.deb"

# HTWicket (https://github.com/elonen/htwicket): nginx auth_request gateway + .htpasswd
# user manager. Replaces the old PHP 'htadmin' for login and user management.
HTWICKET_RELEASE="0.1.0"
HTWICKET_LINK="https://github.com/elonen/htwicket/releases/download/v${HTWICKET_RELEASE}/htwicket_${HTWICKET_RELEASE}-1_${DEBIAN_VER}_${DEB_ARCH}.deb"

# -------------------------------------------------------
# Some helpers

display_config() {
    # Show a config file in a gray background box
    set +x
    local config_file="$1"
    if [[ ! -f "$config_file" ]]; then
        echo "Error: File not found - $config_file"
        return 1
    fi
    echo -e "\e[30;42m$config_file\e[0m"
    echo -e "\e[30;47m"
    while IFS= read -r line; do
        echo -e "    $line"
    done < "$config_file"
    echo -e "\e[0m" # Resetting the color
    set -x
}

restart_and_check_service() {
    # Restart systemd daemon and make sure it worked
    local service_name="$1"
    systemctl enable "$service_name"
    systemctl restart "$service_name"
    systemctl status "$service_name" || { echo "ERROR: $service_name failed to start."; exit 1; }
}

replace_cfg() {
    # Replace one line of config in given file using sed
    local file_path="$1"
    local pattern="$2"
    local replacement="$3"
    sed -i -E "s#$pattern#$replacement#" "$file_path"
}

# -------------------------------------------------------

apt-get install -y gnupg2 wget git sudo acl

# On Bookworm, add the Debian Multimedia repo for a newer FFmpeg.
# (Trixie's stock FFmpeg is recent enough, so the repo isn't needed there.)
# ffmpeg/mediainfo themselves are pulled in as dependencies of the server .deb below.
if [ "${DEBIAN_VER}" = "bookworm" ]; then
  wget -q https://www.deb-multimedia.org/pool/main/d/deb-multimedia-keyring/deb-multimedia-keyring_2016.8.1_all.deb
  dpkg -i deb-multimedia-keyring_2016.8.1_all.deb
  echo "deb https://www.deb-multimedia.org bookworm main non-free" > /etc/apt/sources.list.d/deb-multimedia.list
  apt-get -qy update
fi


# Install other dependencies.
# Python is required by some Clapshot modules (e.g. the organizer); install the
# release-matching interpreter explicitly. ffmpeg/mediainfo are pulled in as
# dependencies of the server .deb below.
apt-get install -y nginx sqlite3 python3 jq "python${PYTHON_VER}"   # For Clapshot

# Make sure data directory is mounted and useable
test -e "$DATA_DIR" || { echo "Data directory '$DATA_DIR' missing. Please mount/create it and run again."; exit 1; }
chown www-data:www-data "$DATA_DIR"
sudo -u www-data mkdir -p "$DATA_DIR/data" || { echo "Data directory not writable by www-data."; exit 1; }

# Get and install the Clapshot .deb packages.
# Using 'apt-get install ./*.deb' (not 'dpkg -i *.deb') so their dependencies
# (ffmpeg, mediainfo, ...) get resolved, and so we don't try to
# (re)install unrelated .deb files left in this directory (e.g. the deb-multimedia keyring).
wget --no-clobber $SERVER_LINK $ORG_LINK $CLIENT_LINK   # Download if necessary
apt-get install -y ./clapshot-*.deb

# Configure server
replace_cfg /etc/clapshot-server.conf   "^url-base.*"   "url-base = $PUBLIC_ADDRESS"   # Set url-base
replace_cfg /etc/clapshot-server.conf   "^data-dir.*"   "data-dir = $DATA_DIR/data"   # Set data-dir
touch /var/log/clapshot.log
chown www-data:www-data /var/log/clapshot.log
display_config /etc/clapshot-server.conf
restart_and_check_service clapshot-server.service

# Configure client
#
# The client (web UI Javascript) needs to know which HTTP(s)/WS(s) URL it can
# reach the server. Set them here.
WS_ADDR=$(echo "$PUBLIC_ADDRESS" | sed 's/^http/ws/')
replace_cfg /etc/clapshot_client.conf   "^ *\"ws_url.*"                            "    \"ws_url\": \"${WS_ADDR}/api/ws\","
replace_cfg /etc/clapshot_client.conf   "^ *\"upload_url.*"                        "    \"upload_url\": \"${PUBLIC_ADDRESS}/api/upload\","
# Disable the old HTTP-Basic-Auth logout hack and add a real Logout link to HTWicket.
# (idempotent: drop any existing /htwicket/logout item before re-adding it)
TMP_CLIENT_CONF=$(mktemp)
jq '.user_menu_show_basic_auth_logout = false
    | .user_menu_extra_items = ([ (.user_menu_extra_items // [])[] | select(.data != "/htwicket/logout") ]
                                + [ {"label":"Logout","type":"url","data":"/htwicket/logout"} ])' \
    /etc/clapshot_client.conf > "$TMP_CLIENT_CONF" && mv "$TMP_CLIENT_CONF" /etc/clapshot_client.conf
display_config /etc/clapshot_client.conf

# Install HTWicket (login + user management). Ships /usr/bin/htwicket, a default
# /etc/htwicket.toml, and a (disabled) systemd unit running as www-data.
wget --no-clobber "$HTWICKET_LINK"   # Download if necessary
apt-get install -y ./htwicket_*.deb

# Remove any leftovers from a previous PHP-htadmin install, so we don't end up with
# two 'default_server' nginx blocks (which would prevent nginx from starting).
rm -f /etc/nginx/sites-enabled/clapshot+htadmin.nginx.conf
rm -rf /var/www/htadmin

# Configure and restart Nginx
# - serve /videos from the data dir
# - set 'server_name' based on your access URL
rm -f /etc/nginx/sites-enabled/default
cp /usr/share/doc/clapshot-client/examples/clapshot+htwicket.nginx.conf /etc/nginx/sites-enabled/
NGINX_CONF="/etc/nginx/sites-enabled/clapshot+htwicket.nginx.conf"
HOSTNAME=$(echo "$PUBLIC_ADDRESS" | sed -E "s|^[^/]*//([^:/]*).*|\\1|")
replace_cfg "$NGINX_CONF"   "^([ \t]*alias).*"         "\\1 ${DATA_DIR}/data/videos;"
replace_cfg "$NGINX_CONF"   "^([ \t]*server_name).*"   "\\1 ${HOSTNAME};"
display_config $NGINX_CONF
restart_and_check_service nginx.service

# Configure HTWicket. Secure cookies require HTTPS, so derive the setting from the
# public URL scheme (a mismatch makes login silently fail as the browser drops the cookie).
case "$PUBLIC_ADDRESS" in
    https://*) HTWICKET_INSECURE_COOKIES="false" ;;
    *)         HTWICKET_INSECURE_COOKIES="true" ;;
esac
cat > /etc/htwicket.toml <<EOF
base_path        = "/htwicket"
htpasswd_file    = "/var/www/.htpasswd"
sidecar_file     = "/var/www/.htwicket.toml"
state_dir        = "/var/lib/htwicket"
listen           = "127.0.0.1:52155"
insecure_cookies = ${HTWICKET_INSECURE_COOKIES}
app_title_html   = "Clapshot"

[superadmins]
expr = "username == 'admin' || fields.is_admin"

[fields.display_name]
type = "string"
default = ""
user_editable_expr = "true"

[fields.is_admin]
type = "bool"
default = false

[fields.can_upload]
type = "bool"
default = true
user_visible = true

[headers.X-Remote-User-Name]
type = "string"
expr = "fields.display_name != '' ? fields.display_name : username"

[headers.X-Remote-User-Is-Admin]
type = "bool"
expr = "username == 'admin' || fields.is_admin"

[headers.X-Remote-User-Can-Upload]
type = "bool"
expr = "fields.can_upload"
EOF
display_config /etc/htwicket.toml

# Per-user fields sidecar (admin is a superadmin). Always (re)written; HTWicket needs
# write access to /var/www to manage users from the web UI.
echo -e "[users.\"admin\"]\nis_admin = true" > /var/www/.htwicket.toml
chown www-data:www-data /var/www /var/www/.htwicket.toml

# Seed users only on a FRESH install (no existing .htpasswd), so upgrades keep the
# admin (and everyone else) you already have - HTWicket reads the same file as before.
ADMIN_PW=""
if [ ! -e /var/www/.htpasswd ]; then
    touch /var/www/.htpasswd
    chown www-data:www-data /var/www/.htpasswd
    # Set up 'admin' user. Use CLAPSHOT_ADMIN_PASSWORD if given, else
    # let HTWicket generate a random bcrypt one and capture it for the banner.
    if [ -n "${CLAPSHOT_ADMIN_PASSWORD}" ] && \
       printf '%s\n' "${CLAPSHOT_ADMIN_PASSWORD}" | sudo -u www-data htwicket user passwd admin 2>/dev/null; then
        ADMIN_PW="${CLAPSHOT_ADMIN_PASSWORD}"
    else
        ADMIN_PW=$(sudo -u www-data htwicket user passwd admin --random | sed -n 's/^generated password: *//p')
    fi
fi
restart_and_check_service htwicket.service

# Show some results
THIS_IP="$(ip addr show | grep 'inet ' | grep -v '127.0.0.1' | awk '{print $2}' | cut -d/ -f1)"     # Get local IPv4 address
set +x
echo "-------------------------------------------------------------"
echo "All done!"
echo "Clapshot should now be accessible at: '${PUBLIC_ADDRESS}', and user management at '${PUBLIC_ADDRESS}/htwicket/admin'"
if [ -n "$ADMIN_PW" ]; then
    echo ""
    echo "  >>> Generated 'admin' password: ${ADMIN_PW}"
    echo "  >>> (write it down now; it is not stored anywhere else)"
else
    echo "Existing /var/www/.htpasswd kept as-is. Log in as your existing 'admin' user to manage users."
    echo "Forgot the admin password? Reset it: sudo -u www-data htwicket user passwd admin --random"
fi
echo "If not reachable, make sure you have configured '${PUBLIC_ADDRESS}' to load data from 'http://${THIS_IP}:80', perhaps by a reverse proxy."
