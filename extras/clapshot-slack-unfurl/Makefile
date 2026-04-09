.PHONY: setup install clean distclean test test-docker debian-local debian-docker debian-docker-single docker

UID := $(shell id -u)
GID := $(shell id -g)

ifeq ($(TARGET_ARCH),)
  ARCH := $(shell uname -m)
  PLATFORM_STR :=
else
  ARCH := $(TARGET_ARCH)
  PLATFORM_STR := --platform linux/$(TARGET_ARCH)
endif

DEBIAN_VER ?= bookworm
ifeq ($(DEBIAN_VER),trixie)
  DEB_PYTHON_VER ?= python3.13
else
  DEB_PYTHON_VER ?= python3.11
endif

# Local venv uses whatever python3 is available; Docker builds use DEB_PYTHON_VER
PYTHON_VER ?= python3

VENV := _venv
PYTHON := $(VENV)/bin/python
PIP := $(VENV)/bin/pip

PY_SRC := $(wildcard clapshot_slack_unfurl/*.py)
DEB_SRC := debian/* README* LICENSE* requirements.txt setup.py pyproject.toml

DOCKER_IMG_NAME := clapshot_slack_unfurl_${DEBIAN_VER}_${ARCH}
DEV_VERSION := $(shell git rev-parse --short HEAD 2>/dev/null || date +"%Y-%m-%d" || echo "dev")

TARGET_BIN := $(VENV)/bin/clapshot-slack-unfurl


$(TARGET_BIN): $(VENV) $(PY_SRC)
	$(PIP) install --no-build-isolation .

$(VENV): requirements.txt requirements-dev.txt
	$(PYTHON_VER) -m venv $(VENV)
	$(PIP) install -U pip
	$(PIP) install -r requirements.txt -r requirements-dev.txt
	@touch $(VENV)

install: $(TARGET_BIN)

test: $(TARGET_BIN)
	$(VENV)/bin/mypy clapshot_slack_unfurl
	$(VENV)/bin/pytest tests/ -v

test-docker: docker
	docker run --rm ${DOCKER_IMG_NAME}:latest bash -c "cd /build && PYTHON_VER=${DEB_PYTHON_VER} make test"

clean:
	rm -rf $(VENV) build dist *.egg-info dist_deb
	find . -type f -name '*.pyc' -delete
	find . -type d -name '__pycache__' -delete

distclean: clean

# --- Debian packaging ---

dist_deb/built.${ARCH}.target: ${DEB_SRC} $(PY_SRC)
	dpkg-buildpackage -us -uc -b
	@mkdir -p dist_deb
	touch $@

debian-local: dist_deb/built.${ARCH}.target

docker:
	DOCKER_BUILDKIT=1 docker build $(PLATFORM_STR) \
		-t ${DOCKER_IMG_NAME}:${DEV_VERSION} \
		--build-arg USER=${USER} --build-arg UID=${UID} --build-arg GID=${GID} \
		--build-arg DEBIAN_VER=${DEBIAN_VER} --build-arg PYTHON_VER=${DEB_PYTHON_VER} \
		-f Dockerfile .
	docker tag ${DOCKER_IMG_NAME}:${DEV_VERSION} ${DOCKER_IMG_NAME}:latest

dist_deb/built.${DEBIAN_VER}.${ARCH}.docker: $(PY_SRC) ${DEB_SRC} Dockerfile
	@mkdir -p dist_deb
	DOCKER_BUILDKIT=1 docker build $(PLATFORM_STR) \
		-t ${DOCKER_IMG_NAME}:${DEV_VERSION} \
		--build-arg USER=${USER} --build-arg UID=${UID} --build-arg GID=${GID} \
		--build-arg DEBIAN_VER=${DEBIAN_VER} --build-arg PYTHON_VER=${DEB_PYTHON_VER} \
		-f Dockerfile .
	docker tag ${DOCKER_IMG_NAME}:${DEV_VERSION} ${DOCKER_IMG_NAME}:latest
	docker run --rm $(PLATFORM_STR) \
		--mount type=bind,source="$$(pwd)/dist_deb",target="/deb" \
		--user root ${DOCKER_IMG_NAME}:latest \
		bash -c 'cd /build && \
			sed -i "s/python3\\.11/${DEB_PYTHON_VER}/g" debian/control && \
			PYTHON_VER=${DEB_PYTHON_VER} make debian-local && \
			for f in dist_deb/*.deb; do \
				newname=$$(echo "$$f" | sed -E "s/(_[0-9.]+-?[0-9]*_)/\1${DEBIAN_VER}_/"); \
				[ "$$f" != "$$newname" ] && mv "$$f" "$$newname" || true; \
			done && \
			cp dist_deb/*.deb /deb/ && chown -R ${UID}:${GID} /deb'
	touch $@

# Build single DEBIAN_VER/ARCH combo (used by the loop below, or directly)
debian-docker-single: dist_deb/built.${DEBIAN_VER}.${ARCH}.docker

# Build all supported Debian versions and architectures
debian-docker:
	@echo "Building Debian packages for multiple distributions..."
	rm -rf dist_deb && mkdir -p dist_deb
	for debver in bookworm trixie; do \
		echo ""; \
		echo "=== Checking availability for Debian $$debver ==="; \
		if docker build --platform linux/amd64 -q - <<< "FROM debian:$$debver-slim" >/dev/null 2>&1; then \
			echo "=== Building packages for Debian $$debver ==="; \
			for plat in arm64 amd64; do \
				echo "--- Building for $$debver/$$plat ---"; \
				DEBIAN_VER=$$debver TARGET_ARCH=$$plat $(MAKE) debian-docker-single; \
			done; \
		else \
			echo "=== Skipping $$debver (base image not available) ==="; \
		fi; \
	done
	rm -f dist_deb/*dbgsym* 2>/dev/null || true
	@echo ""
	@echo "=== Built packages ==="
	ls -l dist_deb/
