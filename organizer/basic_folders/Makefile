.PHONY: setup install clean distclean package

# Configurable paths and settings
VENV := _venv
PYTHON := $(VENV)/bin/python
PIP := $(VENV)/bin/pip

setup:
	@echo "Setting up virtual environment..."
	@test -d $(VENV) || python3 -m venv $(VENV)
	@$(PIP) install -U pip
	@$(PIP) install -r requirements.txt
	@$(PIP) install build

$(VENV): setup

install: $(VENV)
	@echo "Installing the application..."
	@$(PIP) install .

clean:
	@echo "Cleaning up build and Python file artifacts..."
	@rm -rf build dist *.egg-info
	@find . -type f -name '*.pyc' -delete
	@find . -type d -name '__pycache__' -delete

distclean: clean
	@echo "Removing the virtual environment..."
	@rm -rf $(VENV)

package: $(VENV)
	@echo "Packaging the application..."
	@$(PYTHON) -m build

debian-local: dist_deb/built.target

dist_deb/built.target: ${SERVER_CMD} ${DEB_DEPS}
	cargo install cargo-deb
	cargo deb
	mkdir -p dist_deb
	cp target/debian/* dist_deb/
	touch $@

debian-docker:  ${RUST_DEPS} ${DEB_DEPS} ${ENV} docker
	mkdir -p dist_deb
	$(eval PKGDIR=${PACKAGE}-${PVER})
	$(eval CMD="cd /app/server && cp -r --preserve=mode,ownership,timestamps /deb ./dist_deb && ls -l ./dist_deb && make debian-local && cp dist_deb/*.* /deb/ && chown -R ${UID}:${GID} /deb")
	docker run --rm --mount type=bind,source="$$(pwd)/dist_deb",target="/deb" --user root ${PACKAGE}:latest  bash -c ${CMD}

all: install
