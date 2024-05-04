.PHONY: deps clean test docker test run run-local run-docker docs default debian-local

PACKAGE_VERSION=$(shell (git rev-parse --short HEAD || date +"%Y-%m-%d" || echo "dev"))

PLUGIN_BIN=../organizer/default/run-py-org.sh
#PLUGIN_BIN=../organizer/default/target/debug/clapshot-default-organizer
#PLUGIN_BIN="../organizer/basic_folders/run-in-venv.sh"
#../organizer/basic_folders/_venv/bin/clapshot-organizer-basic-folders

UID=$(shell id -u)
GID=$(shell id -g)

ifeq ($(TARGET_ARCH),)
  ARCH=$(shell uname -m)
  PLATFORM_STR =
else
  ARCH = $(TARGET_ARCH)
  PLATFORM_STR = --platform linux/$(TARGET_ARCH)
endif

PACKAGE=clapshot_server_${ARCH}
SERVER_CMD=target/debug/clapshot-server
DEV_DATADIR=DEV_DATADIR

default: ${SERVER_CMD}

${DEV_DATADIR}:
	mkdir -p "$@"

RUST_DEPS=$(shell find src/ migrations/ Cargo.toml diesel.toml -type f) $(shell find ../protobuf/proto -type f)
DEB_DEPS=$(shell find debian/ README* LICENSE*  -type f)

${SERVER_CMD}: ${RUST_DEPS}
	cargo build

clean:
	cargo clean
	rm -rf dist_deb || sudo rm -rf dist_deb
	#cd docs; make proper


docker: ${DEV_DATADIR}
	@echo "-------------------------------------------------------------------------------"
	@echo "NOTE! Creating current user and UID/GID in Docker so bind mount is owned by it."
	@echo "Otherwise you wouldn't be able to access it without root privileges."
	@echo "-------------------------------------------------------------------------------"

	cd .. && DOCKER_BUILDKIT=1 docker build $(PLATFORM_STR) -t ${PACKAGE}:${PACKAGE_VERSION} --build-arg USER=${USER} --build-arg UID=${UID} --build-arg GID=${GID}  -f Dockerfile.server .
	docker tag ${PACKAGE}:${PACKAGE_VERSION} ${PACKAGE}:latest


debian-local: dist_deb/built.target

dist_deb/built.target: ${SERVER_CMD} ${DEB_DEPS}
	cargo install cargo-deb
	cargo deb
	mkdir -p dist_deb
	cp target/debian/* dist_deb/
	touch $@

debian-docker:  ${RUST_DEPS} ${DEB_DEPS} ${ENV} docker
	mkdir -p dist_deb
	$(eval PKGDIR=${PACKAGE}-${PVER})
	$(eval CMD="cd /app/server && cp -r --preserve=mode,ownership,timestamps /deb ./dist_deb && ls -l ./dist_deb && make debian-local && cp dist_deb/*.* /deb/ && chown -R ${UID}:${GID} /deb")
	docker run --rm --mount type=bind,source="$$(pwd)/dist_deb",target="/deb" --user root ${PACKAGE}:latest  bash -c ${CMD}


test-local:
	(cd ../organizer; make)
	TEST_ORG_CMD="${PLUGIN_BIN}" cargo test | grep --line-buffered -vE "(TRACE)|((h2|tower|hyper)\:\:)"
	# TEST_ORG_CMD="${PLUGIN_BIN}" cargo test   test_organizer  | grep --line-buffered -vE "(TRACE)|((h2|tower|hyper)\:\:)"

test-docker: docker
	docker run --rm ${PACKAGE}:latest  cargo test

test:
	@echo "Please run either 'test-docker' or 'test-local'"


docs:
	cd docs; make clean html
	@echo "------------"
	@echo "Documentation generated. See docs/_build/"

run-local: ${SERVER_CMD} ${DEV_DATADIR}
	(cd ../organizer; make)
	${SERVER_CMD} --url-base http://127.0.0.1:8095/ --port 8095 --data-dir ${DEV_DATADIR} --migrate --cors '*' --debug  --org-cmd "${PLUGIN_BIN}"

run-docker: docker
	docker run --rm -it -p 127.0.0.1:8095:8095 --mount type=bind,source="$$(pwd)"/${DEV_DATADIR},target=/app/server/${DEV_DATADIR}  ${PACKAGE}:latest

run:
	@echo "Please run either 'run-docker' or 'run-local'"
