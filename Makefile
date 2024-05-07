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
	(cd client; make debian-docker)
	(cd server; make debian-docker)
	(cd organizer; make debian-docker)
	@if [ "${ARCH}" != "x86_64" ]; then \
		echo "We're running on non-x86_64 architecture. Building x86_64 deb, too."; \
		(cd server; export TARGET_ARCH=amd64; make debian-docker); \
		(cd organizer; export TARGET_ARCH=amd64; make debian-docker); \
	fi
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

#run-docker: clean-debian debian-docker
run-docker: debian-docker
	DOCKER_BUILDKIT=1 docker build -t clapshot-comb --build-arg UID=${UID} --build-arg GID=${GID} -f Dockerfile.demo .
	# Add a simple test video to incoming already
	mkdir -p test/VOLUME/data/incoming
	cp server/src/tests/assets/60fps-example.mp4 test/VOLUME/data/incoming/
	docker run --rm -it -p 0.0.0.0:8080:80 --mount type=bind,source="$$(pwd)"/test/VOLUME,target=/mnt/clapshot-data  clapshot-comb

build-docker-demo: #debian-docker
	@which jq || (echo "ERROR: Please install jq first." && exit 1)
	$(eval PVER=$(shell jq -r '.version' client/package.json))
	docker build -t clapshot:${PVER}-demo --build-arg UID=1002 --build-arg GID=1002 -f Dockerfile.demo .
	docker build -t clapshot:${PVER}-demo-htadmin --build-arg UID=1002 --build-arg GID=1002 -f Dockerfile.demo . --build-arg auth_variation=htadmin

	docker tag clapshot:${PVER}-demo clapshot:latest-demo
	docker tag clapshot:${PVER}-demo-htadmin clapshot:latest-demo-htadmin
	docker tag clapshot:${PVER}-demo elonen/clapshot:${PVER}-demo
	docker tag clapshot:latest-demo elonen/clapshot:latest-demo
	docker tag clapshot:${PVER}-demo-htadmin elonen/clapshot:${PVER}-demo-htadmin
	docker tag clapshot:latest-demo-htadmin elonen/clapshot:latest-demo-htadmin
