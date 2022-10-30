FROM rust AS chef

# Install system packages
RUN apt-get -qy update
RUN apt-get -qy install acl sudo >/dev/null

# Install system packages
RUN apt-get -qy update >/dev/null
RUN apt-get -qy install make >/dev/null
RUN apt-get -qy install ffmpeg >/dev/null
RUN apt-get -qy install mediainfo >/dev/null
RUN apt-get -qy install mscgen >/dev/null
RUN apt-get -qy install sqlite3 >/dev/null

# Rust build deps
RUN apt-get -qy install libssl-dev >/dev/null

# Version of sqlite3 that support ALTER TABLE DROP column (and other new features)
RUN echo 'deb http://ftp.debian.org/debian bookworm main' >> /etc/apt/sources.list.d/bookworm.list
RUN apt-get -qy update >/dev/null
RUN apt-get -qy install -t bookworm sqlite3 >/dev/null
RUN rm /etc/apt/sources.list.d/bookworm.list
RUN apt-get -qy update >/dev/null

# Create regular user
ARG USER=docker
ARG UID=1000
ARG GID=1000
RUN echo "#### USER=${USER}, UID=${UID}, GID=${GID}"
RUN test -n "${USER}" && test -n "${UID}" && test -n "${GID}"

RUN groupadd -f ${USER} --gid=${GID}
RUN useradd -m ${USER} --uid=${UID} --gid=${GID} || true
RUN mkdir -p /app
RUN chown -R ${UID}:${GID} /app

USER ${UID}:${GID}
WORKDIR /app

RUN cargo install cargo-chef
RUN cargo install cargo-deb



FROM chef AS planner
COPY . .
RUN cargo chef prepare  --recipe-path recipe.json



FROM chef AS builder

# Build deps (for Docker layering)
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
RUN rm -f target/release/clapshot-server

# Copy sources & build
COPY . .
RUN make
RUN ls -l /
RUN ls -l /app

EXPOSE 8095
CMD ["make", "run-local"]
