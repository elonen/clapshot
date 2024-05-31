.PHONY: setup install clean distclean package

UID := $(shell id -u)
GID := $(shell id -g)

ifeq ($(TARGET_ARCH),)
  ARCH := $(shell uname -m)
  PLATFORM_STR :=
else
  ARCH := $(TARGET_ARCH)
  PLATFORM_STR := --platform linux/$(TARGET_ARCH)
endif

# Configurable paths and settings
VENV := _venv
PYTHON := $(VENV)/bin/python
PIP := $(VENV)/bin/pip

DEPS_SRC := $(wildcard ../../protobuf/proto/*.proto) $(wildcard ../../protobuf/libs/python/src/**/*.py)
PY_SRC := $(wildcard *.py) $(wildcard organizer/**/*.py) $(DEPS_SRC)

DEB_SRC := debian/* README* LICENSE*

DOCKER_IMG_NAME := clapshot_org_basic_folders_${ARCH}
DEV_VERSION := $(shell git rev-parse --short HEAD || date +"%Y-%m-%d" || echo "dev")

PROTOLIB := clapshot_grpc-0.0.0+dev.tar.gz
TARGET_BIN := $(VENV)/bin/clapshot-organizer-basic-folders


$(TARGET_BIN): $(VENV) $(PY_SRC) $(VENV)/bin/mypy
	$(VENV)/bin/mypy organizer
	$(PIP) install .

$(VENV)/bin/mypy: $(VENV)
	@$(PIP) install mypy==1.9.0
	@touch $@

install: $(TARGET_BIN)

package: $(TARGET_BIN)
	@echo "Packaging the application..."
	@$(PYTHON) -m build --sdist

$(VENV): deps/$(PROTOLIB) requirements.txt
	@echo "Setting up virtual environment..."
	python3 -m venv $(VENV)
	$(PIP) install -U pip
	$(PIP) install -r requirements.txt
	$(PIP) install build
	$(PIP) install --upgrade deps/$(PROTOLIB)
	@touch $(VENV)

deps/$(PROTOLIB): $(DEPS_SRC)
	@echo "Building Clapshot gRPC/proto Python library..."
	cd ../../protobuf/libs/python && make
	mkdir -p deps
	cp ../../protobuf/libs/python/dist/$(PROTOLIB) deps/

clean:
	@echo "Cleaning up build and Python file artifacts..."
	@rm -rf $(VENV)
	@rm -rf deps build dist *.egg-info dist_deb
	@find . -type f -name '*.pyc' -delete
	@find . -type d -name '__pycache__' -delete

distclean: clean
	@echo "Removing the virtual environment..."
	@rm -rf $(VENV)

docker:
	@echo "-------------------------------------------------------------------------------"
	@echo "NOTE! Creating current user and UID/GID in Docker so bind mount is owned by it."
	@echo "Otherwise you wouldn't be able to access it without root privileges."
	@echo "-------------------------------------------------------------------------------"
	cd ../.. && DOCKER_BUILDKIT=1 docker build $(PLATFORM_STR) -t ${DOCKER_IMG_NAME}:${DEV_VERSION} --build-arg USER=${USER} --build-arg UID=${UID} --build-arg GID=${GID} -f Dockerfile.org-python .
	docker tag ${DOCKER_IMG_NAME}:${DEV_VERSION} ${DOCKER_IMG_NAME}:latest

dist_deb/built.${ARCH}.target: ${DEB_SRC} deps/$(PROTOLIB)
	dpkg-buildpackage -us -uc -b
	@mkdir -p dist_deb   # Create the directory if it doesn't exist (in Docker its a link to ../ where the deb is)
	touch $@

debian-local: dist_deb/built.${ARCH}.target

debian-docker: ${PY_SRC} ${DEB_SRC} docker
	@mkdir -p dist_deb
	$(eval CMD="cd /build/organizer/basic_folders && cp -r --preserve=mode,ownership,timestamps /deb ./dist_deb && ls -l ./dist_deb && make debian-local && cp dist_deb/*.* /deb/ && chown -R ${UID}:${GID} /deb")
	docker run --rm --mount type=bind,source="$$(pwd)/dist_deb",target="/deb" --user root ${DOCKER_IMG_NAME}:latest   bash -c ${CMD}
