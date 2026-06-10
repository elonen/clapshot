SHELL := /bin/bash
.PHONY:  clean docker test run-docker run build-docker-demo

UID=$(shell id -u)
GID=$(shell id -g)

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

# Helper function to test if a Docker base image is available
define test_base_image
	@docker build --platform linux/$(1) -q - <<< "FROM rust:1-slim-$(2)" >/dev/null 2>&1
endef

debian-docker:
	@echo "Building Debian packages for multiple distributions..."
	@which jq >/dev/null || (echo "ERROR: Please install jq first." && exit 1)
	$(eval PVER=$(shell jq -r '.version' client/package.json))
	@# Only the collection dir is wiped; component dist_deb dirs (and their
	@# built.*.docker stamps) are kept so unchanged components are skipped.
	rm -rf dist_deb && mkdir -p dist_deb
	set -e; \
	trap 'echo "" >&2; echo "######## debian-docker FAILED while building: $$CURRENT ########" >&2' ERR; \
	for debver in bookworm trixie; do \
		echo ""; \
		echo "=== Checking availability for Debian $$debver ==="; \
		if docker build --platform linux/amd64 -q - <<< "FROM rust:1-slim-$$debver" >/dev/null 2>&1; then \
			echo "=== Building packages for Debian $$debver ==="; \
			for plat in arm64 amd64; do \
				echo "--- Building server for $$debver/$$plat ---"; \
				CURRENT="server $$debver/$$plat"; \
				DEBIAN_VER=$$debver TARGET_ARCH=$$plat make --no-print-directory -C server debian-docker; \
				echo "--- Building organizer for $$debver/$$plat ---"; \
				CURRENT="organizer $$debver/$$plat"; \
				DEBIAN_VER=$$debver TARGET_ARCH=$$plat make --no-print-directory -C organizer debian-docker; \
			done; \
			echo "--- Building client for $$debver ---"; \
			CURRENT="client $$debver"; \
			DEBIAN_VER=$$debver make --no-print-directory -C client debian-docker; \
			echo "--- Collecting $$debver packages ---"; \
			cp client/dist_deb/*$(PVER)*.deb dist_deb/ 2>/dev/null || true; \
			cp server/dist_deb/*$(PVER)*.deb dist_deb/ 2>/dev/null || true; \
			cp organizer/basic_folders/dist_deb/*$(PVER)*.deb dist_deb/ 2>/dev/null || true; \
		else \
			echo "=== Skipping $$debver (base image not available) ==="; \
		fi; \
	done
	rm dist_deb/*dbgsym* 2>/dev/null || true
	@echo ""
	@echo "=== Built packages ==="
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
	docker run --rm -it -p 0.0.0.0:8080:80 \
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
