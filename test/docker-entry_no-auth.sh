#!/bin/bash

# This a Docker-side support script to run a demo of Clapshot (API server + Nginx)
# in a single Docker container for demo and testing purposes.

DIR="/mnt/clapshot-data/data"

# URL_BASE: normalize trailing slash
if [ -z "${CLAPSHOT_SERVER__URL_BASE}" ]; then
    # Try legacy env var CLAPSHOT_URL_BASE first, then default to localhost if not set
    export CLAPSHOT_SERVER__URL_BASE=$(echo "${CLAPSHOT_URL_BASE:-http://127.0.0.1:8080/}" | sed 's#/*$#/#')
else
    export CLAPSHOT_SERVER__URL_BASE=$(echo "${CLAPSHOT_SERVER__URL_BASE}" | sed 's#/*$#/#')
fi

# CORS: try legacy env var CLAPSHOT_CORS first, then default to URL_BASE
if [ -z "${CLAPSHOT_SERVER__CORS}" ]; then
    export CLAPSHOT_SERVER__CORS="${CLAPSHOT_CORS:-${CLAPSHOT_SERVER__URL_BASE}}"
fi

APP_TITLE="${CLAPSHOT_APP_TITLE:-Clapshot}"
LOGO_URL="${CLAPSHOT_LOGO_URL:-clapshot-logo.svg}"

# ============================================================================
# DOCKER ENVIRONMENT VARIABLE TO CONFIG FILE CONVERTER
# ============================================================================
#
# This function allows users to override ANY Clapshot server configuration
# option using Docker environment variables, without needing to mount custom
# config files or rebuild images.
#
# HOW IT WORKS:
# 1. Scans environment for variables starting with CLAPSHOT_SERVER__
# 2. Converts variable names to config file format:
#    CLAPSHOT_SERVER__INGEST_USERNAME_FROM → ingest-username-from
# 3. Updates the config file by either:
#    - Replacing existing values (even if commented out with #)
#    - Adding new options if they don't exist
#
# EXAMPLES:
#   docker run -e CLAPSHOT_SERVER__DEBUG=true image
#   → Sets "debug = true" in config file
#
#   docker run -e CLAPSHOT_SERVER__INGEST_USERNAME_FROM=folder-name image
#   → Sets "ingest-username-from = folder-name" in config file
# ============================================================================
update_config_from_env() {
    local config_file="$1"

    # Function to process environment variable and update config
    process_env_var() {
        local env_var="$1"
        local env_value="$2"
        local prefix="$3"

        # Convert ENV_VAR_NAME to config-key-name
        config_key=$(echo "$env_var" | sed "s/^$prefix//" | tr '[:upper:]' '[:lower:]' | tr '_' '-')

        echo "Setting config: $config_key = $env_value"

        # Escape special characters in the value for sed
        escaped_value=$(printf '%s\n' "$env_value" | sed 's/[[\.*^$(){}?+|/]/\\&/g')

        # Update or add the config option (handle both commented and uncommented)
        if grep -q "^#*$config_key" "$config_file"; then
            # Update existing option (remove # if commented)
            sed -i "s/^#*$config_key.*/$config_key = $escaped_value/g" "$config_file"
        else
            # Add new option under [general] section
            sed -i '/^\[general\]$/a\'"$config_key = $escaped_value" "$config_file"
        fi
    }

    # Process new CLAPSHOT_SERVER__ variables (preferred)
    env | grep '^CLAPSHOT_SERVER__' | while IFS='=' read -r env_var env_value; do
        process_env_var "$env_var" "$env_value" "CLAPSHOT_SERVER__"
    done

    # Process legacy variables for backward compatibility
    # Only process if no CLAPSHOT_SERVER__ equivalent exists
    process_legacy_var() {
        local legacy_var="$1"
        local new_var="CLAPSHOT_SERVER__$(echo "$1" | sed 's/^CLAPSHOT_//')"

        # Only use legacy if new variable is not set
        if [ -n "${!legacy_var}" ] && [ -z "${!new_var}" ]; then
            echo "Using legacy variable $legacy_var (consider migrating to $new_var)"
            process_env_var "$legacy_var" "${!legacy_var}" "CLAPSHOT_"
        fi
    }

    # Backward compatibility for existing variables
    process_legacy_var "CLAPSHOT_URL_BASE"
    process_legacy_var "CLAPSHOT_CORS"
    process_legacy_var "CLAPSHOT_DATA_DIR"
    process_legacy_var "CLAPSHOT_DEBUG"
    process_legacy_var "CLAPSHOT_BITRATE"
}

# Apply all environment variable config overrides (both new and legacy)
update_config_from_env /etc/clapshot-server.conf

# Use the processed URL base for client config
WS_BASE=$(echo "${CLAPSHOT_SERVER__URL_BASE}" | sed 's#^http#ws#')
cat > /etc/clapshot_client.conf << EOF
{
  "ws_url": "${CLAPSHOT_SERVER__URL_BASE}api/ws",
  "upload_url": "${CLAPSHOT_SERVER__URL_BASE}api/upload",
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
---  Browse ${CLAPSHOT_SERVER__URL_BASE}  ---
---
---  ⚠ DEMO / EVALUATION build — single container, NOT for production.
---    See Docker Compose deployment recipes in the repo docs.
==============================================
EOF

set -v

# Dig up start command from systemd script and run it as docker user instead of www-data
CMD=$(grep 'Exec' /lib/systemd/system/clapshot-server.service | sed 's/^.*=//')
sudo -u docker $CMD &

# Follow server log
tail -f /var/log/clapshot.log
