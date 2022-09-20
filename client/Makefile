.PHONY: deps install clean build_clean dist_clean tests dist docker

PACKAGE_VERSION=$(shell (git rev-parse --short HEAD || date +"%Y-%m-%d" || echo "dev"))
PACKAGE=clapshot_client

UID=$(shell id -u)
GID=$(shell id -g)


default: build

clean:
	rm -rf node_modules dist dist_deb

npm-deps:
	npm install vite
	npm install

docker: clean 
	docker build -t ${PACKAGE}:${PACKAGE_VERSION} .
	docker tag ${PACKAGE}:${PACKAGE_VERSION} ${PACKAGE}:latest

docker-for-deb: clean 
	docker build -t ${PACKAGE}:${PACKAGE_VERSION} --build-arg TARGET=deb  .
	docker tag ${PACKAGE}:${PACKAGE_VERSION} ${PACKAGE}:deb


build-local: npm-deps
	npm run build

build-docker: docker-for-deb
	mkdir -p dist
	docker run --rm -it --mount type=bind,source="$$(pwd)"/dist,target=/build/dist  ${PACKAGE}:deb  make build-local
	@echo "==== Build in Docker done. Results are in ./dist/ ===="

build:
	@echo "Run explicitly either 'build-local' or 'build-docker'"


debian-local:
	apt-get -y install make debhelper devscripts
	dpkg-buildpackage -us -uc -b

debian-docker: docker-for-deb
	@which jq || (echo "ERROR: Please install jq first." && exit 1)
	rm -rf dist_deb
	mkdir -p dist_deb
	$(eval PVER="$(shell jq -r '.version' package.json)")
	test -n ${PVER}
	$(eval PKGDIR=${PACKAGE}-${PVER})
	$(eval CMD="mkdir -p /deb/ && cp -a /build /deb/${PKGDIR} && cd /deb/${PKGDIR} && make debian-local && rm -rf /deb/${PKGDIR} && chown -R ${UID}:${GID} /deb")
	docker run -it --rm --mount type=bind,source="$$(pwd)/dist_deb",target="/deb" --user root ${PACKAGE}:deb  bash -c ${CMD}



dev-local: npm-deps
	npm run dev -- --host --port 5173

dev-docker: docker
	docker run --rm -it -p 127.0.0.1:5173:5173  ${PACKAGE}:latest

dev:
	@echo "Run either 'dev-local' or 'dev-docker'"

