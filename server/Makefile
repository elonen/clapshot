.PHONY: deps clean test docker test run run-local run-docker docs default debian-local

PACKAGE_VERSION=$(shell (git rev-parse --short HEAD || date +"%Y-%m-%d" || echo "dev"))
PLUGIN_BIN=../organizer-plugin/simple_rust/target/debug/clapshot-organizer_rust-simple

UID=$(shell id -u)
GID=$(shell id -g)

PACKAGE=clapshot_server
SERVER_CMD=target/debug/clapshot-server
DEV_DATADIR=DEV_DATADIR

default: ${SERVER_CMD}

${DEV_DATADIR}:
	mkdir -p "$@"

RUST_DEPS=$(shell find src/ migrations/ Cargo.toml diesel.toml -type f)
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

	DOCKER_BUILDKIT=1 docker build -t ${PACKAGE}:${PACKAGE_VERSION} --build-arg USER=${USER} --build-arg UID=${UID} --build-arg GID=${GID} .
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
	$(eval CMD="cd /app && cp -r --preserve=mode,ownership,timestamps /deb ./dist_deb && ls -l ./dist_deb && make debian-local && cp dist_deb/* /deb/ && chown -R ${UID}:${GID} /deb")
	docker run --rm --mount type=bind,source="$$(pwd)/dist_deb",target="/deb" --user root ${PACKAGE}:latest  bash -c ${CMD}


test-local:
	cargo test

test-docker: docker
	docker run --rm ${PACKAGE}:latest  cargo test

test:
	@echo "Please run either 'test-docker' or 'test-local'"


docs:
	cd docs; make clean html
	@echo "------------"
	@echo "Documentation generated. See docs/_build/"

run-local: ${SERVER_CMD} ${DEV_DATADIR}
	(cd ../organizer-plugin; make)
	${SERVER_CMD} --url-base http://127.0.0.1:8095/ --port 8095 --org-cmd "${PLUGIN_BIN}" --data-dir ${DEV_DATADIR} --migrate --debug

run-docker: docker
	docker run --rm -it -p 127.0.0.1:8095:8095 --mount type=bind,source="$$(pwd)"/${DEV_DATADIR},target=/app/${DEV_DATADIR}  ${PACKAGE}:latest

run:
	@echo "Please run either 'run-docker' or 'run-local'"
