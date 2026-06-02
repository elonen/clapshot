#!/bin/bash
set -e

# This script install Clapshot with Htadmin Basic auth
# on Debian 12 Bookworm (might also work on Bullseye).
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

set -x

RELEASE="0.11.0"
CLIENT_LINK="https://github.com/elonen/clapshot/releases/download/v${RELEASE}/clapshot-client_${RELEASE}_bookworm_all.deb"
# Detect system architecture and adjust SERVER_LINK accordingly
ARCH=$(uname -m)
case "$ARCH" in
    x86_64)
      SERVER_LINK="https://github.com/elonen/clapshot/releases/download/v${RELEASE}/clapshot-server_${RELEASE}-1_bookworm_amd64.deb"
      ORG_LINK="https://github.com/elonen/clapshot/releases/download/v${RELEASE}/clapshot-organizer-basic-folders_${RELEASE}_bookworm_amd64.deb"
    ;;
    aarch64)
      SERVER_LINK="https://github.com/elonen/clapshot/releases/download/v${RELEASE}/clapshot-server_${RELEASE}-1_bookworm_arm64.deb"
      ORG_LINK="https://github.com/elonen/clapshot/releases/download/v${RELEASE}/clapshot-organizer-basic-folders_${RELEASE}_bookworm_arm64.deb"
    ;;
    *) echo "Unsupported architecture: $ARCH"; exit 1;;
esac

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

# Set up Debian Multimedia repo for newer FFmpeg
apt-get install -y gnupg2 wget git sudo acl
wget -q https://www.deb-multimedia.org/pool/main/d/deb-multimedia-keyring/deb-multimedia-keyring_2016.8.1_all.deb
dpkg -i deb-multimedia-keyring_2016.8.1_all.deb
echo "deb https://www.deb-multimedia.org bookworm main non-free" > /etc/apt/sources.list.d/deb-multimedia.list
apt-get -qy update
apt-get -qy install ffmpeg mediainfo

# Install other dependencies
apt-get install -y nginx python3 python3.11 sqlite3    # For Clapshot
apt-get install -y php-fpm                             # For installation and Htadmin

# Make sure data directory is mounted and useable
test -e "$DATA_DIR" || { echo "Data directory '$DATA_DIR' missing. Please mount/create it and run again."; exit 1; }
chown www-data:www-data "$DATA_DIR"
sudo -u www-data mkdir -p "$DATA_DIR/data" || { echo "Data directory not writable by www-data."; exit 1; }

# Get the Clapshot .deb packages
wget --no-clobber $SERVER_LINK $ORG_LINK $CLIENT_LINK   # Download if necessary
dpkg -i *.deb

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
replace_cfg /etc/clapshot_client.conf   "^ *\"user_menu_show_basic_auth_logout.*"  "    \"user_menu_show_basic_auth_logout\": true"
display_config /etc/clapshot_client.conf

# Configure and restart Nginx
# - serve /videos from the data dir
# - set 'server_name' based on your access URL
# - setup PHP for Htadmin (not required by Clapshot itself)
rm -f /etc/nginx/sites-enabled/default
cp /usr/share/doc/clapshot-client/examples/clapshot+htadmin.nginx.conf /etc/nginx/sites-enabled/
NGINX_CONF="/etc/nginx/sites-enabled/clapshot+htadmin.nginx.conf"
HOSTNAME=$(echo "$PUBLIC_ADDRESS" | sed -E "s|^[^/]*//([^:/]*).*|\\1|")
replace_cfg "$NGINX_CONF"   "^([ \t]*alias).*"         "\\1 ${DATA_DIR}/data/videos;"
replace_cfg "$NGINX_CONF"   "^([ \t]*server_name).*"   "\\1 ${HOSTNAME};"
replace_cfg "$NGINX_CONF"  "^([ \t]*fastcgi_pass).*"  "\\1 unix:/var/run/php/php-fpm.sock;"
display_config $NGINX_CONF
restart_and_check_service nginx.service

# Install Htadmin, a simple password manager
if [ ! -e "htadmin" ]; then git clone https://github.com/soster/htadmin.git; fi
rm -rf /var/www/htadmin
cp -a htadmin/app/htadmin /var/www/htadmin
chown -R www-data:www-data /var/www/htadmin
echo -e "alice:J/JsbnRtaHBlc\ndemo:N7HpG2DddhtME\nadmin:KURMbfRvhQPWs" > /var/www/.htpasswd   # alice:alice123, demo:demo, admin:admin
chown www-data:www-data /var/www/.htpasswd
mv /var/www/htadmin/config/config.ini.example /var/www/htadmin/config/config.ini

# Configure htadmin
replace_cfg /var/www/htadmin/config/config.ini  "secure_path *=.*"       "secure_path = /var/www/"
replace_cfg /var/www/htadmin/config/config.ini  "app_title *= .*"        "app_title = Clapshot users"
replace_cfg /var/www/htadmin/config/config.ini  "mail_server *= .*"      "mail_server = localhost"
replace_cfg /var/www/htadmin/config/config.ini  "admin_user *= .*"       "admin_user = htadmin"
replace_cfg /var/www/htadmin/config/config.ini  "admin_pwd_hash *= .*"   "admin_pwd_hash = Askg15BrpF11g"

# Show some results
THIS_IP="$(ip addr show | grep 'inet ' | grep -v '127.0.0.1' | awk '{print $2}' | cut -d/ -f1)"		# Get local IPv4 address
set +x
echo "-------------------------------------------------------------"
echo "All done!"
echo "Clapshot should now be accessible at: '${PUBLIC_ADDRESS}', and Htadmin at '${PUBLIC_ADDRESS}/htadmin'"
echo "If not, make sure you have configured '${PUBLIC_ADDRESS}' to load data from 'http://${THIS_IP}:80', perhaps by a reverse proxy."