FROM debian:bullseye-slim

# Install system packages
RUN apt-get -qy update

RUN apt-get -qy install python3.9 python3-venv
RUN apt-get -qy install ffmpeg
RUN apt-get -qy install python3-pymediainfo
RUN apt-get -qy install nginx
RUN apt-get -qy install acl sudo

# Version of sqlite3 that support ALTER TABLE DROP column
RUN echo 'deb http://ftp.debian.org/debian bookworm main' >> /etc/apt/sources.list.d/bookworm.list
RUN apt-get -qy update
RUN apt-get -qy install -t bookworm sqlite3
RUN rm /etc/apt/sources.list.d/bookworm.list
RUN apt-get -qy update


# Add regular user (to match local user ID)
ARG UID=1000
ARG GID=1000
RUN echo "### UID=${UID}, GID=${GID}"
RUN test -n "${UID}" && test -n "${GID}"
RUN groupadd -f docker --gid=${GID}
RUN useradd -m docker --uid=${UID} --gid=${GID} || true
RUN mkdir -p /mnt/clapshot-data
RUN chown -R ${UID} /mnt/clapshot-data

# Install Clapshot server & client
COPY dist_deb/clapshot-client_*.deb /root/
COPY dist_deb/clapshot-server_*.deb /root/
RUN dpkg --path-include '/usr/share/doc/*' --refuse-downgrade -i /root/*.deb

WORKDIR /mnt/clapshot-data
EXPOSE 80
COPY test/docker-entry.sh /root/
CMD ["bash", "/root/docker-entry.sh"]
