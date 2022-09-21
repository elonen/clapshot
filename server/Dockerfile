FROM python:3.9-slim


# Install system packages
RUN apt-get -qy update
RUN apt-get -qy install make
RUN apt-get -qy install ffmpeg

# ...for build deb
RUN apt-get -qy install build-essential debhelper devscripts equivs
RUN apt-get -qy install python3-venv python3-dev python-dev dh-virtualenv python3-setuptools libffi-dev


# Create regular user

ARG USER=docker
ARG UID=1000
ARG GID=1000
RUN echo "#### USER=${USER}, UID=${UID}, GID=${GID}"
RUN test -n "${USER}" && test -n "${UID}" && test -n "${GID}"

RUN groupadd -f ${USER} --gid=${GID}
RUN useradd -m ${USER} --uid=${UID} --gid=${GID} || true
RUN mkdir -p /app
RUN chown ${UID}:${GID} /app

# Keeps Python from generating .pyc files in the container
ENV PYTHONDONTWRITEBYTECODE=1
# Turn off buffering for easier container logging
ENV PYTHONUNBUFFERED=1


WORKDIR /app
COPY requirements.txt /app/
USER ${UID}:${GID}
RUN python3.9 -m venv _venv
RUN _venv/bin/pip install --upgrade pip
RUN _venv/bin/pip install wheel
RUN _venv/bin/pip install -r requirements.txt


COPY --chown=${UID}:${GID} . /app
RUN make install
RUN ls -l /
RUN ls -l /app

EXPOSE 8095
CMD ["make", "run-local"]
