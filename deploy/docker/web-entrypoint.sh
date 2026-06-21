#!/bin/sh -e
# Render the runtime client config served as /clapshot_client.conf.json
# (symlink -> /etc/clapshot_client.conf, shipped by the client .deb).
# Runs in nginx:stable-alpine, which has /bin/sh (busybox) but no bash.
: "${CLAPSHOT_URL_BASE:=http://127.0.0.1:8080/}"

# The logout link is auth-specific -> injected by the recipe (htwicket sets CLAPSHOT_LOGOUT_URL).
logout='[]'
if [ -n "${CLAPSHOT_LOGOUT_URL:-}" ]; then
    logout="[{\"label\":\"Logout\",\"type\":\"url\",\"data\":\"${CLAPSHOT_LOGOUT_URL}\"}]"
fi

cat > /etc/clapshot_client.conf <<EOF
{
  "ws_url": "${CLAPSHOT_URL_BASE}api/ws",
  "upload_url": "${CLAPSHOT_URL_BASE}api/upload",
  "user_menu_extra_items": ${logout},
  "user_menu_show_basic_auth_logout": false,
  "logo_url": "${CLAPSHOT_LOGO_URL:-clapshot-logo.svg}",
  "app_title": "${CLAPSHOT_APP_TITLE:-Clapshot}"
}
EOF
