FROM node:18.9.0-slim

# Install system packages
RUN apt-get -qy update
RUN apt-get -qy install make

ARG TARGET=dev
RUN if [ "$TARGET" = "deb" ]; then apt-get -qy install make debhelper devscripts; fi

# Create regular user

ARG USER=docker
ARG UID=1000
ARG GID=1000
RUN echo "#### USER=${USER}, UID=${UID}, GID=${GID}"
RUN test -n "${USER}" && test -n "${UID}" && test -n "${GID}"

RUN groupadd -f ${USER} --gid=${GID}
RUN useradd -m ${USER} --uid=${UID} --gid=${GID} || true
RUN mkdir -p /build
RUN chown ${UID}:${GID} /build


WORKDIR /build
USER ${UID}:${GID}

COPY --chown=${UID}:${GID} package*.json /build/
RUN npm install

COPY --chown=${UID}:${GID} . /build

EXPOSE 5173
CMD ["make", "dev-local"]
