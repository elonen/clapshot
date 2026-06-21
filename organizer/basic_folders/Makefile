.PHONY: setup install clean distclean package get-cur-ver verify-ver

UID := $(shell id -u)
GID := $(shell id -g)

ifeq ($(TARGET_ARCH),)
  ARCH := $(shell uname -m)
  PLATFORM_STR :=
else
  ARCH := $(TARGET_ARCH)
  PLATFORM_STR := --platform linux/$(TARGET_ARCH)
endif

# Debian version and Python version (can be overridden via environment)
DEBIAN_VER ?= bookworm
ifeq ($(DEBIAN_VER),trixie)
  PYTHON_VER ?= python3.13
else
  PYTHON_VER ?= python3.11
endif

# Configurable paths and settings
VENV := _venv
PYTHON := $(VENV)/bin/python
PIP := $(VENV)/bin/pip

DEPS_SRC := $(wildcard ../../protobuf/proto/*.proto) $(wildcard ../../protobuf/libs/python/src/**/*.py)
PY_SRC := $(wildcard *.py) $(wildcard organizer/**/*.py) $(DEPS_SRC)

DEB_SRC := debian/* README* LICENSE* requirements.txt setup.py pyproject.toml

DOCKER_IMG_NAME := clapshot_org_basic_folders_${DEBIAN_VER}_${ARCH}
DEV_VERSION := $(shell git rev-parse --short HEAD || date +"%Y-%m-%d" || echo "dev")

PROTOLIB := clapshot_grpc-0.0.0+dev-py3-none-any.whl
TARGET_BIN := $(VENV)/bin/clapshot-organizer-basic-folders


$(TARGET_BIN): $(VENV) $(PY_SRC) $(VENV)/bin/mypy
	$(VENV)/bin/mypy organizer example_metaplugins
	$(PIP) install --no-build-isolation .

$(VENV)/bin/mypy: $(VENV)
	@$(PIP) install mypy==1.9.0
	@touch $@

install: $(TARGET_BIN)

get-cur-ver:
	@grep -m1 'version=' setup.py | sed -E "s/.*version='([^']+)'.*/\1/"

verify-ver:
	@man=$$($(MAKE) -s get-cur-ver); \
	chg=$$(sed -nE '1s/.*\(([^)]+)\).*/\1/p' debian/changelog | sed -E 's/^[0-9]+://; s/-[^-]*$$//'); \
	test "$$man" = "$$chg" || { echo "ERROR: organizer version mismatch: setup.py=$$man changelog=$$chg"; exit 1; }; \
	echo "organizer version OK: $$man"

package: $(TARGET_BIN)
	@echo "Packaging the application..."
	@$(PYTHON) -m build --sdist

$(VENV): deps/$(PROTOLIB) requirements.txt
	@echo "Setting up virtual environment..."
	python3 -m venv $(VENV)
	$(PIP) install -U pip
	$(PIP) install -r requirements.txt
	$(PIP) install build
	$(PIP) install --upgrade deps/$(PROTOLIB)
	@touch $(VENV)

deps/$(PROTOLIB): $(DEPS_SRC)
	@echo "Building Clapshot gRPC/proto Python library..."
	cd ../../protobuf/libs/python && make
	mkdir -p deps
	cp ../../protobuf/libs/python/dist/$(PROTOLIB) deps/

clean:
	@echo "Cleaning up build and Python file artifacts..."
	@rm -rf $(VENV)
	@rm -rf deps build dist *.egg-info dist_deb
	@find . -type f -name '*.pyc' -delete
	@find . -type d -name '__pycache__' -delete

distclean: clean
	@echo "Removing the virtual environment..."
	@rm -rf $(VENV)

docker:
	@echo "-------------------------------------------------------------------------------"
	@echo "NOTE! Creating current user and UID/GID in Docker so bind mount is owned by it."
	@echo "Otherwise you wouldn't be able to access it without root privileges."
	@echo "-------------------------------------------------------------------------------"
	cd ../.. && DOCKER_BUILDKIT=1 docker build $(PLATFORM_STR) -t ${DOCKER_IMG_NAME}:${DEV_VERSION} --build-arg USER=${USER} --build-arg UID=${UID} --build-arg GID=${GID} --build-arg DEBIAN_VER=${DEBIAN_VER} --build-arg PYTHON_VER=${PYTHON_VER} -f Dockerfile.org-python .
	docker tag ${DOCKER_IMG_NAME}:${DEV_VERSION} ${DOCKER_IMG_NAME}:latest

dist_deb/built.${ARCH}.target: ${DEB_SRC} deps/$(PROTOLIB)
	dpkg-buildpackage -us -uc -b
	@mkdir -p dist_deb   # Create the directory if it doesn't exist (in Docker its a link to ../ where the deb is)
	touch $@

test-local: $(TARGET_BIN)
	# Organizer depends on Clapshot Server's gRPC calls and context, so tests are initialized by it, over actual gRPC session
	cd ../../server && FILTER=org_test_ make test-local

debian-local: dist_deb/built.${ARCH}.target

debian-docker: dist_deb/built.${DEBIAN_VER}.${ARCH}.docker

dist_deb/built.${DEBIAN_VER}.${ARCH}.docker: ${PY_SRC} ${DEB_SRC} ../../Dockerfile.org-python
	@echo "-------------------------------------------------------------------------------"
	@echo "Building organizer for Debian ${DEBIAN_VER} (${ARCH}) with ${PYTHON_VER}"
	@echo "NOTE! Creating current user and UID/GID in Docker so bind mount is owned by it."
	@echo "Otherwise you wouldn't be able to access it without root privileges."
	@echo "-------------------------------------------------------------------------------"
	@mkdir -p dist_deb
	cd ../.. && DOCKER_BUILDKIT=1 docker build $(PLATFORM_STR) -t ${DOCKER_IMG_NAME}:${DEV_VERSION} --build-arg USER=${USER} --build-arg UID=${UID} --build-arg GID=${GID} --build-arg DEBIAN_VER=${DEBIAN_VER} --build-arg PYTHON_VER=${PYTHON_VER} -f Dockerfile.org-python .
	docker tag ${DOCKER_IMG_NAME}:${DEV_VERSION} ${DOCKER_IMG_NAME}:latest
	docker run --rm $(PLATFORM_STR) --mount type=bind,source="$$(pwd)/dist_deb",target="/deb" --user root ${DOCKER_IMG_NAME}:latest \
		bash -c "cd /build/organizer/basic_folders && cp -r --preserve=mode,ownership,timestamps /deb ./dist_deb && \
			sed -i 's/python3\\.11/${PYTHON_VER}/g' debian/control && \
			PYTHON_VER=${PYTHON_VER} make debian-local && cp dist_deb/*.deb /deb/ && chown -R ${UID}:${GID} /deb"
	@# Tag freshly-built .deb files with the Debian version, inserting it just
	@# before the arch suffix. Already-tagged debs (this or another distro) keep
	@# their codename, so cross-distro artifacts left in dist_deb aren't re-tagged.
	for f in dist_deb/*.deb; do \
		newname=$$(echo "$$f" | sed -E 's/(_[0-9.]+-?[0-9]*)_(amd64|arm64|all)\.deb$$/\1_${DEBIAN_VER}_\2.deb/'); \
		[ "$$f" != "$$newname" ] && mv "$$f" "$$newname" || true; \
	done
	touch $@
