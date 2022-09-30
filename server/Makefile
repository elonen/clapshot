.PHONY: deps install clean wheel test docker dev test run docs

ENV=_venv
PYTHON=python3
SITE_PACKAGES=${ENV}/lib/${PYTHON}/site-packages
IN_ENV=. ${ENV}/bin/activate;
PYTHON_ENV=${ENV}/bin/${PYTHON}
PACKAGE_VERSION=$(shell (git rev-parse --short HEAD || date +"%Y-%m-%d" || echo "dev"))

UID=$(shell id -u)
GID=$(shell id -g)

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

clean:
	rm -rf "${ENV}" dist build *.egg-info
	rm -rf dist_deb || sudo rm -rf dist_deb
	cd docs; make proper
	# find . -name __pycache__ -type d -exec rm -r {} ';'



venv: ${ENV}

install: ${SERVER_CMD}

dev: ${ENV}
	${PYTHON_ENV} -m pip install --editable .


docker: clean ${DEV_DATADIR}
	@echo "-------------------------------------------------------------------------------------------------------------------"
	@echo "NOTE! Creating your current username and UID/GID insode Docker so bind mounted ${DEV_DATADIR} is owned by you."
	@echo "Otherwise you wouldn't be able to access it without root privileges."
	@echo "-------------------------------------------------------------------------------------------------------------------"

	DOCKER_BUILDKIT=1 docker build -t ${PACKAGE}:${PACKAGE_VERSION} --build-arg USER=${USER} --build-arg UID=${UID} --build-arg GID=${GID} .
	docker tag ${PACKAGE}:${PACKAGE_VERSION} ${PACKAGE}:latest

debian-local: clean wheel
	apt-get -qy install build-essential debhelper devscripts equivs
	apt-get -qy install python3-venv python3-dev python-dev dh-virtualenv python3-setuptools libffi-dev
	dpkg-buildpackage -us -uc -b

debian-docker: docker ${ENV}
	rm -rf dist_deb
	mkdir -p dist_deb
	$(eval PVER="$(shell ${PYTHON_ENV} setup.py --version)")
	test -n ${PVER}
	$(eval PKGDIR=${PACKAGE}-${PVER})
	$(eval CMD="mkdir -p /deb/ && cp -a /app /deb/${PKGDIR} && cd /deb/${PKGDIR} && make debian-local && rm -rf /deb/${PKGDIR} && chown -R ${UID}:${GID} /deb")
	docker run --rm --mount type=bind,source="$$(pwd)/dist_deb",target="/deb" --user root ${PACKAGE}:latest  bash -c ${CMD}


test-local: dev
	${IN_ENV} pytest --test-alembic

test-docker: docker
	docker run --rm ${PACKAGE}:latest make test-local

test:
	@echo "Please run either 'test-docker' or 'test-local'"


docs:
	cd docs; make clean html
	cd docs; make man
	@echo "------------"
	@echo "Documentation generated. See docs/_build/"

run-local: dev ${DEV_DATADIR}
	${SERVER_CMD} --url-base http://127.0.0.1:8095/ --port 8095 --data-dir ${DEV_DATADIR} --host-videos --debug -m aiosqlite

run-docker: docker
	docker run --rm -it -p 127.0.0.1:8095:8095 --mount type=bind,source="$$(pwd)"/${DEV_DATADIR},target=/app/${DEV_DATADIR}  ${PACKAGE}:latest

run:
	@echo "Please run either 'run-docker' or 'run-local'"
