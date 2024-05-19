PACKAGE_NAME := clapshot_grpc
PACKAGE_VERSION := 0.0.0+dev

TARGET_FILE := dist/$(PACKAGE_NAME)-$(PACKAGE_VERSION).tar.gz

VENV := _venv
PYTHON := $(VENV)/bin/python
PIP := $(VENV)/bin/pip
MYPY := $(VENV)/bin/mypy

PROTO_DIR := ../../proto
PROTO_FILES := $(wildcard $(PROTO_DIR)/*.proto)
PROTO_BASENAMES := $(notdir $(PROTO_FILES))

PACKAGE_DIR := src/$(PACKAGE_NAME)
GENERATED_DIR := $(PACKAGE_DIR)/proto

PY_SRC := $(wildcard *.py) $(wildcard src/**/*.py) $(wildcard ../../protobuf/proto/*.proto)


$(TARGET_FILE): $(PY_SRC) $(VENV) $(MYPY) $(GENERATED_DIR)/__init__.py
	$(MYPY) src --no-strict-optional
	$(PYTHON) -m pip install build
	$(PYTHON) -m build --sdist
	@echo "------- Build complete -------"
	@echo "pip install dist/$(PACKAGE_NAME)-$(PACKAGE_VERSION).tar.gz"

package: $(TARGET_FILE)

$(VENV): requirements.txt
	@echo "Setting up virtual environment..."
	python3 -m venv $(VENV)
	$(PIP) install -U pip
	$(PIP) install -r requirements.txt
	$(PIP) install build
	@touch $(VENV)


$(MYPY): $(VENV)
	@$(PIP) install mypy

$(GENERATED_DIR)/__init__.py: $(VENV) $(PROTO_FILES)
	$(PYTHON) -m grpc_tools.protoc --plugin=$(abspath $(VENV)/bin/protoc-gen-python_betterproto) \
	  --python_betterproto_out=$(GENERATED_DIR) --experimental_allow_proto3_optional \
	  --proto_path=$(PROTO_DIR) $(PROTO_BASENAMES)

# Make venv
activate: $(VENV)/bin/activate requirements.txt
$(VENV)/bin/activate:
	python3 -m venv $(VENV)
	$(VENV)/bin/pip install -r requirements.txt

clean:
	# Remove generated files, but keep __init__.py
	rm -rf $(GENERATED_DIR)/*

	# Remove build artifacts
	rm -rf $(VENV)
	rm -rf dist

.PHONY: default clean activate package protobufs
