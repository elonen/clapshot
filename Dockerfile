ARG auth_variation=none

FROM debian:bullseye-slim AS base

# Install system packages
RUN apt-get -qy update

RUN apt-get -qy install python3 >/dev/null
RUN apt-get -qy install ffmpeg >/dev/null
RUN apt-get -qy install mediainfo >/dev/null
RUN apt-get -qy install nginx >/dev/null
RUN apt-get -qy install acl sudo >/dev/null

# Version of sqlite3 that support ALTER TABLE DROP column
RUN echo 'deb http://ftp.debian.org/debian bookworm main' >> /etc/apt/sources.list.d/bookworm.list
RUN apt-get -qy update >/dev/null
RUN apt-get -qy install -t bookworm sqlite3 >/dev/null
RUN rm /etc/apt/sources.list.d/bookworm.list
RUN apt-get -qy update >/dev/null


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
RUN echo "Current architecture: $(dpkg --print-architecture)"
RUN dpkg --path-include '/usr/share/doc/*' --refuse-downgrade -i /root/*_$(dpkg --print-architecture).deb /root/*_all.deb

RUN rm -f /etc/nginx/sites-enabled/*

# ------------- no auth (default demo) -------------

FROM base AS auth-none
RUN echo "### auth-none"
RUN cp /usr/share/doc/clapshot-client/examples/clapshot.nginx.conf  /etc/nginx/sites-enabled/clapshot
COPY test/docker-entry_no-auth.sh /root/docker-entry.sh


# ------------- basic auth (with PHP htadmin for management) -------------

FROM base AS base-git-php
RUN apt-get -qy install git php7.4-fpm >/dev/null
RUN mkdir -p /run/php


FROM base-git-php AS auth-htadmin
RUN echo "### auth-htadmin"
RUN cp /usr/share/doc/clapshot-client/examples/clapshot+htadmin.nginx.conf /etc/nginx/sites-enabled/clapshot

RUN git clone https://github.com/soster/htadmin.git
RUN cp -r htadmin/app/htadmin /var/www/htadmin
RUN chown -R www-data:www-data /var/www/htadmin

RUN echo "alice:J/JsbnRtaHBlc\ndemo:N7HpG2DddhtME\nadmin:KURMbfRvhQPWs" > /var/www/.htpasswd   # alice:alice123, demo:demo, admin:admin
RUN chown www-data:www-data /var/www/.htpasswd
RUN mv /var/www/htadmin/config/config.ini.example /var/www/htadmin/config/config.ini

RUN sed -i 's@secure_path *=.*@secure_path = /var/www/@' /var/www/htadmin/config/config.ini
RUN sed -i 's@app_title *= .*@app_title = Clapshot users@' /var/www/htadmin/config/config.ini
RUN sed -i 's@mail_server *= .*@mail_server = localhost@' /var/www/htadmin/config/config.ini
RUN sed -i 's@admin_user *= .*@admin_user = htadmin@' /var/www/htadmin/config/config.ini
RUN sed -i 's@admin_pwd_hash *= .*@admin_pwd_hash = Askg15BrpF11g@' /var/www/htadmin/config/config.ini

COPY test/docker-entry_htadmin.sh /root/docker-entry.sh


# -------------

FROM auth-${auth_variation} AS final
WORKDIR /mnt/clapshot-data
EXPOSE 80
CMD ["bash", "/root/docker-entry.sh"]
