.PHONY: dev docker build clean dist docker npm-deps npm-check-local npm-test-local docker-deb debian-docker debian-local test-docker build-local dev-local dev-docker default

PACKAGE_VERSION=$(shell (git rev-parse --short HEAD || date +"%Y-%m-%d" || echo "dev"))
PACKAGE=clapshot_client

UID=$(shell id -u)
GID=$(shell id -g)

# Debian version (can be overridden via environment)
DEBIAN_VER ?= bookworm

CLIENT_DEPS=$(shell find src/ public/ package*.json tsconfig.json vite.config.ts -type f 2>/dev/null) $(shell find ../protobuf/proto -type f)
DEB_DEPS=$(shell find debian/ -type f)


default: build-local

clean:
	rm -rf node_modules dist dist_deb
	cd ../protobuf/libs/typescript && make clean

npm-deps:
	npm install vite
	npm install
	cd ../protobuf/libs/typescript && make

docker: clean
	cd .. && DOCKER_BUILDKIT=1 docker build -t ${PACKAGE}:${PACKAGE_VERSION} --build-arg DEBIAN_VER=${DEBIAN_VER} -f Dockerfile.client .
	docker tag ${PACKAGE}:${PACKAGE_VERSION} ${PACKAGE}:latest



npm-check-local: npm-deps
	npm run check

npm-test-local: npm-deps
	npm run test:run

build-local: npm-deps npm-check-local npm-test-local
	npm run build


debian-local:
	apt-get -qy install make debhelper devscripts
	dpkg-buildpackage -us -uc -b

docker-deb:
	cd .. && DOCKER_BUILDKIT=1 docker build -t ${PACKAGE}:${PACKAGE_VERSION} --build-arg TARGET=deb --build-arg DEBIAN_VER=${DEBIAN_VER} -f Dockerfile.client .
	docker tag ${PACKAGE}:${PACKAGE_VERSION} ${PACKAGE}:deb

debian-docker: dist_deb/built.${DEBIAN_VER}.docker

dist_deb/built.${DEBIAN_VER}.docker: ${CLIENT_DEPS} ${DEB_DEPS} ../Dockerfile.client
	@echo "Building client for Debian ${DEBIAN_VER}"
	@which jq || (echo "ERROR: Please install jq first." && exit 1)
	mkdir -p dist_deb
	$(MAKE) docker-deb
	$(eval PVER=$(shell jq -r '.version' package.json))
	test -n ${PVER}
	$(eval PKGDIR=${PACKAGE}-${PVER})
	docker run --rm --mount type=bind,source="$$(pwd)/dist_deb",target="/deb" --user root ${PACKAGE}:deb \
		bash -c "mkdir -p /deb/ && cp -a /build /deb/${PKGDIR} && cd /deb/${PKGDIR}/client && make debian-local && cp /deb/${PKGDIR}/*.deb /deb/ && rm -rf /deb/${PKGDIR} && chown -R ${UID}:${GID} /deb"
	@# Tag freshly-built .deb files with the Debian version, inserting it just
	@# before the arch suffix. Already-tagged debs (this or another distro) keep
	@# their codename, so cross-distro artifacts left in dist_deb aren't re-tagged.
	for f in dist_deb/*.deb; do \
		newname=$$(echo "$$f" | sed -E 's/(_[0-9.]+-?[0-9]*)_(amd64|arm64|all)\.deb$$/\1_${DEBIAN_VER}_\2.deb/'); \
		[ "$$f" != "$$newname" ] && mv "$$f" "$$newname" || true; \
	done
	touch $@

dev-local: npm-deps
	npm run dev -- --host --port 5173

dev-docker: docker
	docker run --rm -it -p 127.0.0.1:5173:5173  ${PACKAGE}:latest

dev:
	@echo "Run either 'dev-local' or 'dev-docker'"

test-docker: docker-deb
	docker run --rm ${PACKAGE}:deb bash -c "cd /build/client && make npm-check-local && make npm-test-local"
	@echo "=== TypeScript checks and tests completed successfully ===="
