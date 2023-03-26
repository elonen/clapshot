.PHONY: all clean bin_path

# Extract the package name, binary name, and all Rust dependencies from Cargo.toml
PACKAGE=$(shell grep -m1 "^name\s*=" Cargo.toml | sed 's/name\s*=\s*"\(.*\)"/\1/')
BIN=target/debug/$(shell cargo metadata --format-version 1 | jq -r '.packages[] | select(.name == "${PACKAGE}") | .targets[] | select(.kind[] == "bin") | .name')
RUST_DEPS=$(shell find src/ Cargo.toml -type f)

${BIN}: ${RUST_DEPS}
	cargo build

bin_path:
	@echo ${BIN}

clean:
	cargo clean

all: ${BIN}
