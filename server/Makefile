.PHONY: deps clean test docker test run run-local run-docker default debian-local
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

DOCKER_IMG_NAME=clapshot_server_${ARCH}
DEV_VERSION=$(shell (git rev-parse --short HEAD || date +"%Y-%m-%d" || echo "dev"))

SERVER_CMD=target/debug/clapshot-server
DEV_DATADIR=DEV_DATADIR

default: ${SERVER_CMD}

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

	cd .. && DOCKER_BUILDKIT=1 docker build $(PLATFORM_STR) -t ${DOCKER_IMG_NAME}:${DEV_VERSION} --build-arg USER=${USER} --build-arg UID=${UID} --build-arg GID=${GID}  -f Dockerfile.server .
	docker tag ${DOCKER_IMG_NAME}:${DEV_VERSION} ${DOCKER_IMG_NAME}:latest


debian-local: dist_deb/built.${ARCH}.target

dist_deb/built.${ARCH}.target: ${SERVER_CMD} ${DEB_DEPS}
	cargo install cargo-deb
	cargo deb
	mkdir -p dist_deb
	cp target/debian/* dist_deb/
	touch $@

debian-docker: docker
	mkdir -p dist_deb
	$(eval CMD="cd /app/server && cp -r --preserve=mode,ownership,timestamps /deb ./dist_deb && ls -l ./dist_deb && make debian-local && cp dist_deb/*.* /deb/ && chown -R ${UID}:${GID} /deb")
	docker run --rm --mount type=bind,source="$$(pwd)/dist_deb",target="/deb" --user root ${DOCKER_IMG_NAME}:latest  bash -c ${CMD}


test-local:
	(cd ../organizer; make)

	@if [ "${CI}" ]; then \
		echo "!! Running in CI: No-default-features = skip expensive tests"; \
		cargo test --no-default-features | grep --line-buffered -vE "(TRACE)|((h2|tower|hyper)\:\:)"; \
	else \
		cargo test | grep --line-buffered -vE "(TRACE)|((h2|tower|hyper)\:\:)"; \
	fi

	@echo "---------- Testing Organizer ----------"
	@# Run Organizer tests through Server (so it tests database and gRPC realistically).
	@# The `cargo test test_organizer` writes the log to a file, so we'll create a temp file for it.
	@LOGFILE=$$(mktemp -u); \
	tail -F "$$LOGFILE" 2>/dev/null & tail_pid=$$!; \
	set -o pipefail; \
	TEST_ORG_CMD="../organizer/basic_folders/run-py-org.sh" \
	TEST_ORG_LOG="$$LOGFILE" \
	cargo test test_organizer | grep --line-buffered -vE "(TRACE)|((h2|tower|hyper)\:\:)" || { status=$$?; kill $$tail_pid; rm "$$LOGFILE"; exit $$status; }; \
	kill $$tail_pid 2>/dev/null; \
	rm -f -- "$$LOGFILE"


test-docker: docker
	docker run --rm -e CI="${CI}" ${DOCKER_IMG_NAME}:latest make test-local

test:
	@echo "Please run either 'test-docker' or 'test-local'"


run-local: ${SERVER_CMD} ${DEV_DATADIR}
	(cd ../organizer; make)
	${SERVER_CMD} --url-base http://127.0.0.1:8095/ --port 8095 --data-dir ${DEV_DATADIR} --migrate --cors '*' --org-cmd "${PLUGIN_BIN}" -d --default-user "admin"

run-docker: docker
	docker run --rm -it -p 127.0.0.1:8095:8095 --mount type=bind,source="$$(pwd)"/${DEV_DATADIR},target=/app/server/${DEV_DATADIR}  ${DOCKER_IMG_NAME}:latest

run:
	@echo "Please run either 'run-docker' or 'run-local'"
