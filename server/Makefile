.PHONY: deps install clean build_clean dist_clean tests dist docker

ENV=_venv
PYTHON=python3.9
SITE_PACKAGES=${ENV}/lib/${PYTHON}/site-packages
IN_ENV=. ${ENV}/bin/activate;
PYTHON_ENV=${ENV}/bin/${PYTHON}
PACKAGE_VERSION=$(shell (git rev-parse --short HEAD || date +"%Y-%m-%d" || echo "dev"))

PACKAGE=clapshot_server
SERVER_CMD=${ENV}/bin/clapshot-server
DEV_DATADIR=DEV_DATADIR

default: ${ENV}

${ENV}:
	@echo "Creating Python environment '$@'..." >&2
	${PYTHON} -m venv "$@"
	@echo "Updating pip..." >&2
	${PYTHON_ENV} -m pip install -U pip setuptools
	@echo "=== Virtual env '${ENV}' ready. Use 'source ${@}/bin/activate' to activate it."
	@echo " "

${DEV_DATADIR}:
	mkdir -p "$@"

${SERVER_CMD}: ${ENV}
	${PYTHON_ENV} -m pip install .



wheel: ${ENV}
	${PYTHON_ENV} -m pip install -U wheel
	${PYTHON_ENV} setup.py bdist_wheel

dist: wheel

dist_clean:
	rm -rf dist

build_clean:
	rm -rf build

clean: build_clean dist_clean
	rm -rf "${ENV}" dist build __pycache__ *.egg-info



venv: ${ENV}

install: ${SERVER_CMD}

install_dev: ${ENV}
	${PYTHON_ENV} -m pip install --editable .


docker: build_clean dist_clean ${DEV_DATADIR}
	@echo "-------------------------------------------------------------------------------------------------------------------"
	@echo "NOTE! Creating your current username and UID/GID insode Docker so bind mounted ${DEV_DATADIR} is owned by you."
	@echo "Otherwise you wouldn't be able to access it without root privileges."
	@echo "-------------------------------------------------------------------------------------------------------------------"

	docker build -t ${PACKAGE}:${PACKAGE_VERSION} --build-arg USER=${USER} --build-arg UID=$$(id -u) --build-arg GID=$$(id -g) .
	docker tag ${PACKAGE}:${PACKAGE_VERSION} ${PACKAGE}:latest


test-local: install_dev
	${IN_ENV} pytest

test-docker: docker
	docker run --rm -it ${PACKAGE}:latest make test-local

test:
	@echo "Please run either 'test-docker' or 'test-local'"



run-local: ${SERVER_CMD} ${DEV_DATADIR}
	${SERVER_CMD} --url-base http://127.0.0.1:8095/ --port 8095 --data-dir ${DEV_DATADIR} --host-videos --debug -m aiosqlite

run-docker: docker
	docker run --rm -it -p 127.0.0.1:8095:8095 --mount type=bind,source="$$(pwd)"/${DEV_DATADIR},target=/app/${DEV_DATADIR}  ${PACKAGE}:latest

run:
	@echo "Please run either 'run-docker' or 'run-local'"
