.PHONY:  clean docker test run-docker run build-docker-demo

UID=$(shell id -u)
GID=$(shell id -g)

CLAPSHOT_DOCKER_HOST ?= 127.0.0.1
CLAPSHOT_DOCKER_PORT ?= 8080
CLAPSHOT_URL_BASE ?= http://$(CLAPSHOT_DOCKER_HOST):$(CLAPSHOT_DOCKER_PORT)/

ifeq ($(TARGET_ARCH),)
  ARCH=$(shell uname -m)
  PLATFORM_STR =
else
  ARCH = $(TARGET_ARCH)
  PLATFORM_STR = --platform linux/$(TARGET_ARCH)
endif


default:
	@echo "Make target 'debian-docker' explicitly."


clean-debian:
	rm -rf dist_deb

debian-docker:
	for plat in arm64 amd64; do \
		cd server; TARGET_ARCH=$$plat make debian-docker; cd ..; \
		cd organizer; TARGET_ARCH=$$plat make debian-docker; cd ..; \
	done
	cd client && make debian-docker
	mkdir -p dist_deb
	cp client/dist_deb/* dist_deb/
	cp server/dist_deb/* dist_deb/
	cp organizer/basic_folders/dist_deb/* dist_deb/
	rm dist_deb/*dbgsym* dist_deb/built.*.* 2>/dev/null || true
	ls -l dist_deb/

clean:	clean-debian
	(cd client; make clean)
	(cd server; make clean)
	(cd organizer; make clean)
	(cd protobuf; make clean)

docker:
	(cd client; make docker)
	(cd server; make docker)
	(cd organizer; make docker)

test:
	(cd client; make test-docker)
	(cd server; make test-docker)


run-docker: debian-docker
	DOCKER_BUILDKIT=1 docker build -t clapshot-comb --build-arg UID=${UID} --build-arg GID=${GID} --pull -f Dockerfile.demo .
	# Add a simple test video to incoming already
	mkdir -p test/VOLUME/data/incoming
	cp server/src/tests/assets/60fps-example.mp4 test/VOLUME/data/incoming/
	@echo "Removing any existing Unix socket files for macOS Docker compatibility..."
	rm -f test/VOLUME/data/grpc-srv-to-org.sock test/VOLUME/data/grpc-org-to-srv.sock
	docker run --rm -it -p $(CLAPSHOT_DOCKER_HOST):$(CLAPSHOT_DOCKER_PORT):80 \
		-e CLAPSHOT_SERVER__URL_BASE=$(CLAPSHOT_URL_BASE) \
		--mount type=bind,source="$$(pwd)"/test/VOLUME,target=/mnt/clapshot-data \
		--mount type=bind,source="$$(pwd)"/organizer/basic_folders/example_metaplugins,target=/opt/clapshot-org-bf-metaplugins,readonly \
		clapshot-comb


build-docker-demo: debian-docker
	@which jq || (echo "ERROR: Please install jq first." && exit 1)
	$(eval PVER=$(shell jq -r '.version' client/package.json))
	DOCKER_BUILDKIT=1 docker build --platform linux/amd64,linux/arm64 --pull \
		-t clapshot:${PVER}-demo \
		-t elonen/clapshot:${PVER}-demo \
		-t elonen/clapshot:latest-demo \
		--build-arg UID=1002 --build-arg GID=1002 -f Dockerfile.demo .

	DOCKER_BUILDKIT=1 docker build --platform linux/amd64,linux/arm64 --pull \
		-t clapshot:${PVER}-demo-htadmin \
		-t elonen/clapshot:${PVER}-demo-htadmin \
		-t elonen/clapshot:latest-demo-htadmin \
		--build-arg UID=1002 --build-arg GID=1002 -f Dockerfile.demo . --build-arg auth_variation=htadmin


build-docker-demo-and-push-hub: debian-docker
	@which jq || (echo "ERROR: Please install jq first." && exit 1)
	$(eval PVER=$(shell jq -r '.version' client/package.json))

	DOCKER_BUILDKIT=1 docker build --platform linux/amd64,linux/arm64 --pull \
		-t elonen/clapshot:${PVER}-demo \
		-t elonen/clapshot:latest-demo \
		--build-arg UID=1002 --build-arg GID=1002 -f Dockerfile.demo \
		--push .

	DOCKER_BUILDKIT=1 docker build --platform linux/amd64,linux/arm64 --pull \
		-t elonen/clapshot:${PVER}-demo-htadmin \
		-t elonen/clapshot:latest-demo-htadmin \
		--build-arg UID=1002 --build-arg GID=1002 -f Dockerfile.demo --build-arg auth_variation=htadmin \
		--push .

build-docker-dev: debian-docker
	@which jq || (echo "ERROR: Please install jq first." && exit 1)
	$(eval GIT_COMMIT=$(shell git rev-parse --short HEAD))
	
	DOCKER_BUILDKIT=1 docker build --platform linux/amd64,linux/arm64 --pull \
		-t elonen/clapshot:git-${GIT_COMMIT}-demo \
		--build-arg UID=1002 --build-arg GID=1002 -f Dockerfile.demo . --build-arg auth_variation=htadmin

build-docker-dev-and-push-hub: debian-docker
	@which jq || (echo "ERROR: Please install jq first." && exit 1)
	$(eval GIT_COMMIT=$(shell git rev-parse --short HEAD))
	
	DOCKER_BUILDKIT=1 docker build --platform linux/amd64,linux/arm64 --pull \
		-t elonen/clapshot:git-${GIT_COMMIT}-demo \
		--build-arg UID=1002 --build-arg GID=1002 -f Dockerfile.demo --build-arg auth_variation=htadmin \
		--push .
