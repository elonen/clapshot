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

all: install
