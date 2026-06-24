SHELL := /bin/bash
.PHONY:  clean docker test run-docker run build-docker-demo verify-ver

UID=$(shell id -u)
GID=$(shell id -g)

CLIENT_VER := $(shell $(MAKE) -s -C client get-cur-ver)
SERVER_VER := $(shell $(MAKE) -s -C server get-cur-ver)
ORG_VER    := $(shell $(MAKE) -s -C organizer/basic_folders get-cur-ver)

ifeq ($(TARGET_ARCH),)
  ARCH=$(shell uname -m)
  PLATFORM_STR =
else
  ARCH = $(TARGET_ARCH)
  PLATFORM_STR = --platform linux/$(TARGET_ARCH)
endif

# Host arch => Debian arch (uname's x86_64/aarch64/arm64 -> amd64/arm64)
HOST_DEB_ARCH := $(shell uname -m | sed 's/x86_64/amd64/; s/aarch64/arm64/')


default:
	@echo "Make target 'debian-docker' explicitly."


clean-debian:
	rm -rf dist_deb


.PHONY: debian-docker debian-docker-one debian-docker-test

# Build a single (DEBIAN_VER x TARGET_ARCH) set of .debs into dist_deb/
debian-docker-one: verify-ver
	@which jq >/dev/null || (echo "ERROR: Please install jq first." && exit 1)
	@test -n "$(DEBIAN_VER)" && test -n "$(TARGET_ARCH)" || \
		(echo "ERROR: debian-docker-one needs DEBIAN_VER and TARGET_ARCH" && exit 1)
	mkdir -p dist_deb
	@echo "--- Building server for $(DEBIAN_VER)/$(TARGET_ARCH) ---"
	DEBIAN_VER=$(DEBIAN_VER) TARGET_ARCH=$(TARGET_ARCH) $(MAKE) --no-print-directory -C server debian-docker
	@echo "--- Building organizer for $(DEBIAN_VER)/$(TARGET_ARCH) ---"
	DEBIAN_VER=$(DEBIAN_VER) TARGET_ARCH=$(TARGET_ARCH) $(MAKE) --no-print-directory -C organizer debian-docker
	@echo "--- Building client for $(DEBIAN_VER) ---"
	DEBIAN_VER=$(DEBIAN_VER) $(MAKE) --no-print-directory -C client debian-docker
	@echo "--- Collecting $(DEBIAN_VER)/$(TARGET_ARCH) packages ---"
	cp client/dist_deb/*$(CLIENT_VER)*$(DEBIAN_VER)*.deb dist_deb/ 2>/dev/null || true
	cp server/dist_deb/*$(SERVER_VER)*$(DEBIAN_VER)*.deb dist_deb/ 2>/dev/null || true
	cp organizer/basic_folders/dist_deb/*$(ORG_VER)*$(DEBIAN_VER)*.deb dist_deb/ 2>/dev/null || true
	rm -f dist_deb/*dbgsym* 2>/dev/null || true

# Full matrix: build every Debian release x Arch variant
debian-docker:
	@echo "Building Debian packages for multiple distributions..."
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
				CURRENT="$$debver/$$plat"; \
				DEBIAN_VER=$$debver TARGET_ARCH=$$plat $(MAKE) --no-print-directory debian-docker-one; \
			done; \
		else \
			echo "Error: Rust base image for $$debver not available, aborting."; \
			exit 1; \
		fi; \
	done
	@echo ""
	@echo "=== Built packages ==="
	ls -l dist_deb/

# Current arch trixie packages for CI testing
debian-docker-for-test: verify-ver
	$(MAKE) --no-print-directory debian-docker-one DEBIAN_VER=trixie TARGET_ARCH=$(HOST_DEB_ARCH)

clean:	clean-debian
	(cd client; make clean)
	(cd server; make clean)
	(cd organizer; make clean)
	(cd protobuf; make clean)

docker:
	(cd client; make docker)
	(cd server; make docker)
	(cd organizer; make docker)

test: debian-docker-for-test
	(cd client; make test-docker)
	(cd server; make test-docker)
	deploy/compose/tests/run.sh

verify-ver:
	@$(MAKE) -s -C client verify-ver
	@$(MAKE) -s -C server verify-ver
	@$(MAKE) -s -C organizer/basic_folders verify-ver

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


# The demo image bundles all components; tag it with the client (product) version.
.PHONY: build-docker-demo-and-push-hub push-demo-hub build-docker-dev build-docker-dev-and-push-hub
build-docker-demo: debian-docker
	DOCKER_BUILDKIT=1 docker build --platform linux/amd64,linux/arm64 --pull \
		-t clapshot:${CLIENT_VER}-demo \
		-t elonen/clapshot:${CLIENT_VER}-demo \
		-t elonen/clapshot:latest-demo \
		--build-arg UID=1002 --build-arg GID=1002 -f Dockerfile.demo .

	DOCKER_BUILDKIT=1 docker build --platform linux/amd64,linux/arm64 --pull \
		-t clapshot:${CLIENT_VER}-demo-htwicket \
		-t elonen/clapshot:${CLIENT_VER}-demo-htwicket \
		-t elonen/clapshot:latest-demo-htwicket \
		--build-arg UID=1002 --build-arg GID=1002 -f Dockerfile.demo . --build-arg auth_variation=htwicket


push-demo-hub:
	DOCKER_BUILDKIT=1 docker build --platform linux/amd64,linux/arm64 --pull \
		-t elonen/clapshot:${CLIENT_VER}-demo \
		-t elonen/clapshot:latest-demo \
		--build-arg UID=1002 --build-arg GID=1002 -f Dockerfile.demo \
		--push .

	DOCKER_BUILDKIT=1 docker build --platform linux/amd64,linux/arm64 --pull \
		-t elonen/clapshot:${CLIENT_VER}-demo-htwicket \
		-t elonen/clapshot:latest-demo-htwicket \
		--build-arg UID=1002 --build-arg GID=1002 -f Dockerfile.demo --build-arg auth_variation=htwicket \
		--push .

build-docker-demo-and-push-hub: debian-docker push-demo-hub

build-docker-dev: debian-docker
	@which jq || (echo "ERROR: Please install jq first." && exit 1)
	$(eval GIT_COMMIT=$(shell git rev-parse --short HEAD))

	DOCKER_BUILDKIT=1 docker build --platform linux/amd64,linux/arm64 --pull \
		-t elonen/clapshot:git-${GIT_COMMIT}-demo \
		--build-arg UID=1002 --build-arg GID=1002 -f Dockerfile.demo . --build-arg auth_variation=htwicket

build-docker-dev-and-push-hub: debian-docker
	@which jq || (echo "ERROR: Please install jq first." && exit 1)
	$(eval GIT_COMMIT=$(shell git rev-parse --short HEAD))

	DOCKER_BUILDKIT=1 docker build --platform linux/amd64,linux/arm64 --pull \
		-t elonen/clapshot:git-${GIT_COMMIT}-demo \
		--build-arg UID=1002 --build-arg GID=1002 -f Dockerfile.demo --build-arg auth_variation=htwicket \
		--push .


# ---- Per-service runtime images for the deploy/compose recipes (ghcr.io) ----
# clapshot-server (server + organizer) and clapshot-web (nginx + client SPA), built from
# the same dist_deb/ packages. Caddy and htwicket are pulled as stock/external images.
GHCR_NS ?= ghcr.io/elonen

.PHONY: build-docker-services build-docker-services-and-push-ghcr push-services-ghcr

# Per-service images are tagged with their primary component's version:
# clapshot-server (server + bundled organizer) -> SERVER_VER, clapshot-web (client SPA) -> CLIENT_VER.
build-docker-services: debian-docker
	DOCKER_BUILDKIT=1 docker build --platform linux/amd64,linux/arm64 --pull \
		-t $(GHCR_NS)/clapshot-server:${SERVER_VER} -t $(GHCR_NS)/clapshot-server:latest \
		-f deploy/docker/clapshot-server.Dockerfile .
	DOCKER_BUILDKIT=1 docker build --platform linux/amd64,linux/arm64 --pull \
		-t $(GHCR_NS)/clapshot-web:${CLIENT_VER} -t $(GHCR_NS)/clapshot-web:latest \
		-f deploy/docker/clapshot-web.Dockerfile .

push-services-ghcr:
	DOCKER_BUILDKIT=1 docker build --platform linux/amd64,linux/arm64 --pull \
		-t $(GHCR_NS)/clapshot-server:${SERVER_VER} -t $(GHCR_NS)/clapshot-server:latest \
		-f deploy/docker/clapshot-server.Dockerfile --push .
	DOCKER_BUILDKIT=1 docker build --platform linux/amd64,linux/arm64 --pull \
		-t $(GHCR_NS)/clapshot-web:${CLIENT_VER} -t $(GHCR_NS)/clapshot-web:latest \
		-f deploy/docker/clapshot-web.Dockerfile --push .

build-docker-services-and-push-ghcr: debian-docker push-services-ghcr


# ---- Publish everything (release convenience) ----
# Image registry policy:
#   - Compose per-service images (clapshot-server, clapshot-web) -> GitHub Container Registry
#   - All-in-one demo/eval images (clapshot:*-demo[-htwicket])   -> Docker Hub
# (htwicket and Caddy are external images, published by their own projects.)
# debian-docker is a shared prerequisite, so the .debs are built once for both pushes.
.PHONY: publish-images
publish-images: build-docker-services-and-push-ghcr build-docker-demo-and-push-hub
	@echo "Published per-service images to $(GHCR_NS) and demo images to Docker Hub (elonen/clapshot)."
