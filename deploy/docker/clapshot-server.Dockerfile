# Clapshot API server + basic-folders organizer, built from the .debs in dist_deb/.
# The server spawns the organizer itself (org-cmd), so they share this container.
# Build context = repo root:  docker build -f deploy/docker/clapshot-server.Dockerfile .
FROM debian:trixie-slim

RUN apt-get -qy update && \
    apt-get -qy install --no-install-recommends \
        ca-certificates acl sqlite3 ffmpeg mediainfo && \
    rm -rf /var/lib/apt/lists/*
# Trixie ships a recent ffmpeg, so no deb-multimedia repo is needed.

COPY dist_deb/*.deb /tmp/deb/
RUN A=$(dpkg --print-architecture); \
    dpkg -i /tmp/deb/clapshot-server_*_trixie_$A.deb \
            /tmp/deb/clapshot-organizer-basic-folders_*_trixie_$A.deb && \
    rm -rf /tmp/deb

# Data dir lives on a mounted volume; make the mountpoint www-data-owned so a fresh
# named volume initializes with the right ownership (bind mounts: chown to uid 33 yourself).
RUN mkdir -p /mnt/clapshot-data/data && chown -R www-data /mnt/clapshot-data && \
    chown www-data /etc/clapshot-server.conf

COPY deploy/docker/server-entrypoint.sh /usr/local/bin/server-entrypoint
COPY deploy/docker/config-check.sh      /usr/local/bin/clapshot-config-check
RUN chmod +x /usr/local/bin/server-entrypoint /usr/local/bin/clapshot-config-check

EXPOSE 8095
USER www-data
ENTRYPOINT ["/usr/local/bin/server-entrypoint"]
# The deb ships run-with-conf.sh: it reads key=value /etc/clapshot-server.conf and turns
# it into clapshot-server CLI args (data-dir, port=8095, org-cmd, ...).
CMD ["/bin/bash", "/usr/share/clapshot-server/run-with-conf.sh", "/etc/clapshot-server.conf"]
