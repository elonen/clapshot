# Clapshot web tier: nginx + the web client (from the client .deb) + the auth-AGNOSTIC
# proxy snippets. Authentication is NOT in this image — each recipe mounts its own
# site.conf at /etc/nginx/conf.d/default.conf.
# Build context = repo root:  docker build -f deploy/docker/clapshot-web.Dockerfile .

# Extract the client SPA from its .deb (trixie has dpkg-deb; the final alpine image doesn't).
FROM debian:trixie-slim AS spa
COPY dist_deb/clapshot-client_*_trixie_all.deb /tmp/client.deb
RUN dpkg-deb -x /tmp/client.deb /spa

FROM nginx:stable-alpine
COPY --from=spa /spa/usr/share/clapshot-client /usr/share/clapshot-client
# Recreate the deb's symlinks (the /var/www one lives outside the path we copied):
RUN mkdir -p /var/www && \
    ln -s /usr/share/clapshot-client/www /var/www/clapshot-client && \
    ln -sf /etc/clapshot_client.conf /var/www/clapshot-client/clapshot_client.conf.json && \
    rm -f /etc/nginx/conf.d/default.conf
# Reusable, auth-agnostic config included by every recipe's site.conf
COPY deploy/docker/snippets/ /etc/nginx/snippets/
# Renders /etc/clapshot_client.conf from env before nginx starts (nginx runs *.sh here)
COPY deploy/docker/web-entrypoint.sh /docker-entrypoint.d/40-clapshot-client-conf.sh
RUN chmod +x /docker-entrypoint.d/40-clapshot-client-conf.sh
