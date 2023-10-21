PACKAGE_NAME := clapshot_grpc
PACKAGE_VERSION := 0.0.1
PACKAGE_DIR := pkg_tmp
DIST_DIR := dist
VENV := _venv
PROTO_DIR := ../../proto

PROTO_FILES = $(wildcard $(PROTO_DIR)/*.proto)
PROTO_BASENAMES = $(notdir $(PROTO_FILES))

VENV_PYTHON := $(abspath $(VENV)/bin/python)
PROTO_FILES := $(wildcard $(PROTO_DIR)/*.proto)
PROTO_GRPC_FILES := organizer.proto

# Build the library (default target)
build: activate $(DIST_DIR) generate package
	@echo "\nBuild completed successfully!"
	@echo "You can install the package using:"
	@echo "pip install $(DIST_DIR)/$(PACKAGE_NAME)-$(PACKAGE_VERSION).tar.gz"

# Make venv
activate: $(VENV)/bin/activate requirements.txt
$(VENV)/bin/activate:
	python3 -m venv $(VENV)
	$(VENV)/bin/pip install -r requirements.txt

$(DIST_DIR):
	mkdir -p $(DIST_DIR)

generate:
	_venv/bin/python -m grpc_tools.protoc --experimental_allow_proto3_optional --python_out=$(DIST_DIR) --proto_path=$(PROTO_DIR) $(PROTO_BASENAMES)
	_venv/bin/python -m grpc_tools.protoc --experimental_allow_proto3_optional --python_out=$(DIST_DIR) --grpc_python_out=$(DIST_DIR) --proto_path=$(PROTO_DIR) $(PROTO_GRPC_FILES)
	# KLUDGE: use `protoletariat` command to make the import relative (see https://github.com/protocolbuffers/protobuf/issues/1491)
	_venv/bin/protol --create-package --in-place --python-out $(DIST_DIR) protoc --proto-path=$(PROTO_DIR) $(PROTO_BASENAMES)

package: protobufs
	mkdir -p $(PACKAGE_DIR)/$(PACKAGE_NAME)
	# Create an __init__.py file for the package
	touch $(PACKAGE_DIR)/$(PACKAGE_NAME)/__init__.py
	cp $(DIST_DIR)/* $(PACKAGE_DIR)/$(PACKAGE_NAME)
	echo "from setuptools import setup, find_packages" > $(PACKAGE_DIR)/setup.py
	echo "setup(name='$(PACKAGE_NAME)', version='$(PACKAGE_VERSION)', packages=find_packages(), install_requires=['protobuf', 'grpcio-tools'])" >> $(PACKAGE_DIR)/setup.py
	cd $(PACKAGE_DIR) && $(VENV_PYTHON) setup.py sdist
	cp $(PACKAGE_DIR)/dist/*.tar.gz $(DIST_DIR)

clean:
	rm -rf $(DIST_DIR)
	rm -rf $(VENV)
	rm -rf $(PACKAGE_DIR)
	rm -rf sdist

.PHONY: build clean activate package protobufs
