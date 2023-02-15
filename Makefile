.PHONY:  clean docker test run-docker run build-docker-demo

UID=$(shell id -u)
GID=$(shell id -g)


default:
	@echo "Make target 'debian-docker' explicitly."


clean-debian:
	rm -rf dist_deb

debian-docker:
	(cd client; make debian-docker)
	(cd server; make debian-docker)
	mkdir -p dist_deb
	cp client/dist_deb/* dist_deb/
	cp server/dist_deb/* dist_deb/
	ls -l dist_deb/
	
clean:	clean-debian
	(cd client; make clean)
	(cd server; make clean)

docker:
	(cd client; make docker)
	(cd server; make docker)

test:
	(cd client; make test-docker)
	(cd server; make test-docker)

#run-docker: clean-debian debian-docker
run-docker: debian-docker
	DOCKER_BUILDKIT=1 docker build -t clapshot-comb --build-arg UID=${UID} --build-arg GID=${GID} .
	# Add a simple test video to incoming already
	mkdir -p test/VOLUME/data/incoming
	cp server/src/tests/assets/60fps-example.mp4 test/VOLUME/data/incoming/
	docker run --rm -it -p 0.0.0.0:8080:80 --mount type=bind,source="$$(pwd)"/test/VOLUME,target=/mnt/clapshot-data  clapshot-comb

build-docker-demo: debian-docker
	@which jq || (echo "ERROR: Please install jq first." && exit 1)
	$(eval PVER="$(shell jq -r '.version' client/package.json)")
	docker build -t clapshot:${PVER}-demo --build-arg UID=1002 --build-arg GID=1002 .
	docker build -t clapshot:${PVER}-demo-htadmin --build-arg UID=1002 --build-arg GID=1002 . --build-arg auth_variation=htadmin
