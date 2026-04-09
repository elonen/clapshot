ARG DEBIAN_VER=bookworm
ARG PYTHON_VER=python3.11
FROM debian:${DEBIAN_VER}-slim AS base

RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends \
        build-essential \
        debhelper \
        dh-virtualenv \
        dh-python \
        python3 \
        python3-dev \
        python3-pip \
        python3-setuptools \
        python3-venv \
        git \
        make \
        ; \
    rm -rf /var/lib/apt/lists/*;

ARG PYTHON_VER=python3.11
RUN apt-get update && apt-get install -y --no-install-recommends ${PYTHON_VER} ${PYTHON_VER}-venv && rm -rf /var/lib/apt/lists/*

FROM base AS build

ARG USER=docker
ARG UID=1000
ARG GID=1000
RUN groupadd -f ${USER} --gid=${GID}
RUN useradd -m ${USER} --uid=${UID} --gid=${GID} || true
RUN mkdir -p /build && chown -R ${UID}:${GID} /build

USER ${UID}:${GID}
COPY --chown=${UID}:${GID} . /build
WORKDIR /build
RUN ln -sf ../ dist_deb
