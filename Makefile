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

debian-docker:
	@for plat in arm64 amd64; do \
		cd server; export TARGET_ARCH=$$plat; make debian-docker; cd ..; \
		cd organizer; export TARGET_ARCH=$$plat; make debian-docker; cd ..; \
	done
	(cd client; make debian-docker)
	mkdir -p dist_deb
	cp client/dist_deb/* dist_deb/
	cp server/dist_deb/* dist_deb/
	cp organizer/basic_folders/dist_deb/* dist_deb/
	rm dist_deb/*dbgsym*
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
	docker run --rm -it -p 0.0.0.0:8080:80 --mount type=bind,source="$$(pwd)"/test/VOLUME,target=/mnt/clapshot-data  clapshot-comb


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
