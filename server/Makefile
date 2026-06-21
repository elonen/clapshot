.PHONY: deps clean test docker test run run-local run-docker default debian-local get-cur-ver verify-ver
SHELL := /bin/bash

PLUGIN_BIN=../organizer/basic_folders/run-py-org.sh

UID=$(shell id -u)
GID=$(shell id -g)

ifeq ($(TARGET_ARCH),)
  ARCH=$(shell uname -m)
  PLATFORM_STR =
else
  ARCH = $(TARGET_ARCH)
  PLATFORM_STR = --platform linux/$(TARGET_ARCH)
endif

# Debian version and Python version (can be overridden via environment)
DEBIAN_VER ?= bookworm
ifeq ($(DEBIAN_VER),trixie)
  PYTHON_VER ?= python3.13
else
  PYTHON_VER ?= python3.11
endif

DOCKER_IMG_NAME=clapshot_server_${DEBIAN_VER}_${ARCH}
DEV_VERSION=$(shell (git rev-parse --short HEAD || date +"%Y-%m-%d" || echo "dev"))

# Persistent build caches (Docker named volumes) so repeated/emulated Docker
# builds don't re-download and recompile every crate from scratch. The cargo
# registry/git are platform-independent and shared across all builds; the
# target dir is keyed per distro+arch so objects from different targets never mix.
CARGO_CACHE_MOUNTS = \
	--mount type=volume,source=clapshot-cargo-registry,target=/usr/local/cargo/registry \
	--mount type=volume,source=clapshot-cargo-git,target=/usr/local/cargo/git \
	--mount type=volume,source=clapshot-target-server-${DEBIAN_VER}-${ARCH},target=/app/server/target

SERVER_CMD=target/debug/clapshot-server
DEV_DATADIR=DEV_DATADIR

DEFAULT_USER ?= admin



default: ${SERVER_CMD}

get-cur-ver:
	@grep -m1 '^version' Cargo.toml | sed -E 's/^version *= *"([^"]+)".*/\1/'

verify-ver:
	@man=$$($(MAKE) -s get-cur-ver); \
	chg=$$(sed -nE '1s/.*\(([^)]+)\).*/\1/p' debian/changelog | sed -E 's/^[0-9]+://; s/-[^-]*$$//'); \
	test "$$man" = "$$chg" || { echo "ERROR: server version mismatch: Cargo.toml=$$man changelog=$$chg"; exit 1; }; \
	echo "server version OK: $$man"

${DEV_DATADIR}:
	mkdir -p "$@"

RUST_DEPS=$(shell find src/ migrations/ Cargo.toml diesel.toml -type f) $(shell find ../protobuf/proto -type f) $(shell find ../protobuf/libs/rust -type f)
DEB_DEPS=$(shell find debian/ README* LICENSE*  -type f)

${SERVER_CMD}: ${RUST_DEPS}
	cargo build

clean:
	cargo clean
	rm -rf dist_deb || sudo rm -rf dist_deb
	rm -f organizer_tests.log

docker: ${DEV_DATADIR}
	@echo "-------------------------------------------------------------------------------"
	@echo "NOTE! Creating current user and UID/GID in Docker so bind mount is owned by it."
	@echo "Otherwise you wouldn't be able to access it without root privileges."
	@echo "-------------------------------------------------------------------------------"
	cd .. && DOCKER_BUILDKIT=1 docker build $(PLATFORM_STR) -t ${DOCKER_IMG_NAME}:${DEV_VERSION} --build-arg USER=${USER} --build-arg UID=${UID} --build-arg GID=${GID} --build-arg DEBIAN_VER=${DEBIAN_VER} --build-arg PYTHON_VER=${PYTHON_VER} -f Dockerfile.server .
	docker tag ${DOCKER_IMG_NAME}:${DEV_VERSION} ${DOCKER_IMG_NAME}:latest

debian-local: dist_deb/built.${ARCH}.target

dist_deb/built.${ARCH}.target: ${RUST_DEPS} ${DEB_DEPS}
	@command -v cargo-deb >/dev/null 2>&1 || cargo install cargo-deb
	cargo deb
	mkdir -p dist_deb
	find target/debian/ -maxdepth 1 -type f -exec cp {} dist_deb/ \;
	touch $@

debian-docker: dist_deb/built.${DEBIAN_VER}.${ARCH}.docker

dist_deb/built.${DEBIAN_VER}.${ARCH}.docker: ${RUST_DEPS} ${DEB_DEPS} ../Dockerfile.server
	@echo "-------------------------------------------------------------------------------"
	@echo "Building for Debian ${DEBIAN_VER} (${ARCH}) with ${PYTHON_VER}"
	@echo "NOTE! Creating current user and UID/GID in Docker so bind mount is owned by it."
	@echo "Otherwise you wouldn't be able to access it without root privileges."
	@echo "-------------------------------------------------------------------------------"
	mkdir -p dist_deb
	cd .. && DOCKER_BUILDKIT=1 docker build $(PLATFORM_STR) -t ${DOCKER_IMG_NAME}:${DEV_VERSION} --build-arg USER=${USER} --build-arg UID=${UID} --build-arg GID=${GID} --build-arg DEBIAN_VER=${DEBIAN_VER} --build-arg PYTHON_VER=${PYTHON_VER} -f Dockerfile.server .
	docker tag ${DOCKER_IMG_NAME}:${DEV_VERSION} ${DOCKER_IMG_NAME}:latest
	docker run --rm $(PLATFORM_STR) --mount type=bind,source="$$(pwd)/dist_deb",target="/deb" $(CARGO_CACHE_MOUNTS) --user root ${DOCKER_IMG_NAME}:latest \
		bash -c "cd /app/server && cp -r --preserve=mode,ownership,timestamps /deb ./dist_deb && make debian-local && cp dist_deb/*.deb /deb/ && chown -R ${UID}:${GID} /deb"
	@# Tag freshly-built .deb files with the Debian version, inserting it just
	@# before the arch suffix. Already-tagged debs (this or another distro) keep
	@# their codename, so cross-distro artifacts left in dist_deb aren't re-tagged.
	for f in dist_deb/*.deb; do \
		newname=$$(echo "$$f" | sed -E 's/(_[0-9.]+-?[0-9]*)_(amd64|arm64|all)\.deb$$/\1_${DEBIAN_VER}_\2.deb/'); \
		[ "$$f" != "$$newname" ] && mv "$$f" "$$newname" || true; \
	done
	touch $@


test-local:
	(cd ../organizer; make)

	@if [ -z "${FILTER}" ]; then \
		echo "!! Running all tests. To run specific tests, use: make test-local FILTER=\"pattern\""; \
	else \
		echo "!! Running tests matching filter: '${FILTER}'"; \
	fi

	@set -o pipefail; if [ "${CI}" ]; then \
		echo "!! Running in CI: No-default-features = skip expensive tests"; \
		cargo test --no-default-features ${FILTER} | grep --line-buffered -vE "(TRACE)|((h2|tower|hyper)\:\:)"; \
	else \
		cargo test ${FILTER} | grep --line-buffered -vE "(TRACE)|((h2|tower|hyper)\:\:)"; \
	fi

	@echo "---------- Testing Organizer ----------"
	@# Run Organizer tests through Server (so it tests database and gRPC realistically).
	@# The `cargo test test_organizer` writes the log to a file, so we'll create a temp file for it.
	@LOGFILE=$$(mktemp); \
	tail_pid=""; \
	cleanup() { \
		if [ -n "$$tail_pid" ] && kill -0 "$$tail_pid" 2>/dev/null; then \
			kill "$$tail_pid" 2>/dev/null || true; \
		fi; \
		rm -f "$$LOGFILE" 2>/dev/null || true; \
	}; \
	trap cleanup EXIT INT TERM; \
	tail -F "$$LOGFILE" 2>/dev/null & tail_pid=$$!; \
	set -o pipefail; \
	TEST_ORG_CMD="../organizer/basic_folders/run-py-org.sh" \
	TEST_ORG_LOG="$$LOGFILE" \
	TEST_ORG_FILTER="${FILTER}" \
	cargo test test_organizer | grep --line-buffered -vE "(TRACE)|((h2|tower|hyper)\:\:)" || exit_code=$$?; \
	cleanup; \
	exit $${exit_code:-0}


test-docker: docker
	docker run --rm -e CI="${CI}" ${DOCKER_IMG_NAME}:latest make test-local

test:
	@echo "Please run either 'test-docker' or 'test-local'"


run-local: ${SERVER_CMD} ${DEV_DATADIR}
	(cd ../organizer; make)
	${SERVER_CMD} --url-base http://127.0.0.1:8095/ --port 8095 --data-dir ${DEV_DATADIR} --migrate --cors '*' --org-cmd "${PLUGIN_BIN}" -d --default-user "${DEFAULT_USER}"

run-docker: docker
	@echo "Removing any existing Unix socket files for macOS Docker compatibility..."
	rm -f ${DEV_DATADIR}/grpc-srv-to-org.sock ${DEV_DATADIR}/grpc-org-to-srv.sock
	docker run --rm -it -p 127.0.0.1:8095:8095 --mount type=bind,source="$$(pwd)"/${DEV_DATADIR},target=/app/server/${DEV_DATADIR}  ${DOCKER_IMG_NAME}:latest

run:
	@echo "Please run either 'run-docker' or 'run-local'"
