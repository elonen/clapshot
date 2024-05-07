PACKAGE_NAME := clapshot_grpc
PACKAGE_VERSION := 0.0.0+dev
VENV := _venv

PROTO_DIR := ../../proto
PROTO_FILES = $(wildcard $(PROTO_DIR)/*.proto)
PROTO_BASENAMES = $(notdir $(PROTO_FILES))

BUILD_DIR := build
PACKAGE_DIR := $(BUILD_DIR)/$(PACKAGE_NAME)

VENV_PYTHON := $(abspath $(VENV)/bin/python)
PROTO_FILES := $(wildcard $(PROTO_DIR)/*.proto)
PROTO_GRPC_FILES := organizer.proto

# Build the library (default target)
default: activate $(BUILD_DIR) package
	@echo "\nBuild completed successfully!"
	@echo "You can install the package using:"
	@echo "pip install $(BUILD_DIR)/dist/$(PACKAGE_NAME)-$(PACKAGE_VERSION).tar.gz"

# Make venv
activate: $(VENV)/bin/activate requirements.txt
$(VENV)/bin/activate:
	python3 -m venv $(VENV)
	$(VENV)/bin/pip install -r requirements.txt

$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)
	mkdir -p $(PACKAGE_DIR)

protobufs:
	$(VENV_PYTHON) -m grpc_tools.protoc --plugin=$(abspath $(VENV)/bin/protoc-gen-python_betterproto) \
	  --python_betterproto_out=$(BUILD_DIR)/$(PACKAGE_NAME) --experimental_allow_proto3_optional \
	  --proto_path=$(PROTO_DIR) $(PROTO_BASENAMES)

#generate-stubs:
#	$(VENV_PYTHON) -m pip install mypy
#	cd $(BUILD_DIR) && ../$(VENV)/bin/stubgen -p $(PACKAGE_NAME) -o ./$(PACKAGE_NAME)

package: protobufs # generate-stubs
	$(VENV_PYTHON) -m pip install build
	echo "from setuptools import setup, find_packages" > $(BUILD_DIR)/setup.py
	echo "setup(name='$(PACKAGE_NAME)', version='$(PACKAGE_VERSION)', packages=find_packages(), install_requires=[], package_data={'$(PACKAGE_NAME)': ['py.typed', '*.pyi']})" >> $(BUILD_DIR)/setup.py
	echo "" > $(BUILD_DIR)/$(PACKAGE_NAME)/py.typed
	cp README.md $(BUILD_DIR)
	cd $(BUILD_DIR) && $(VENV_PYTHON) -m build --sdist

clean:
	rm -rf $(BUILD_DIR)
	rm -rf $(VENV)
	rm -rf $(PACKAGE_DIR)
	rm -rf sdist

.PHONY: default clean activate package protobufs
