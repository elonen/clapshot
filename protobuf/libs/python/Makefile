PACKAGE_NAME := clapshot_grpc
PACKAGE_VERSION := 0.0.1
VENV := _venv

PROTO_DIR := ../../proto
PROTO_FILES = $(wildcard $(PROTO_DIR)/*.proto)
PROTO_BASENAMES = $(notdir $(PROTO_FILES))

DIST_DIR := dist
PACKAGE_DIR := $(DIST_DIR)/$(PACKAGE_NAME)

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
	mkdir -p $(PACKAGE_DIR)

generate:
	$(VENV_PYTHON) -m grpc_tools.protoc --plugin=$(abspath $(VENV)/bin/protoc-gen-python_betterproto) \
	  --python_betterproto_out=$(DIST_DIR)/$(PACKAGE_NAME) --experimental_allow_proto3_optional \
	  --proto_path=$(PROTO_DIR) $(PROTO_BASENAMES)

package: protobufs
	echo "from setuptools import setup, find_packages" > $(DIST_DIR)/setup.py
	echo "setup(name='$(PACKAGE_NAME)', version='$(PACKAGE_VERSION)', packages=find_packages(), install_requires=['betterproto>=2.0.0', 'grpclib'])" >> $(DIST_DIR)/setup.py
	cd $(DIST_DIR) && $(VENV_PYTHON) setup.py sdist
	cp $(DIST_DIR)/dist/*.tar.gz $(DIST_DIR)

clean:
	rm -rf $(DIST_DIR)
	rm -rf $(VENV)
	rm -rf $(PACKAGE_DIR)
	rm -rf sdist

.PHONY: build clean activate package protobufs
