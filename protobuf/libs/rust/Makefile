.PHONY: clean

TARGET=target/debug/makefile.target
RUST_DEPS=$(shell find src/ Cargo.toml -type f)

${TARGET}: ${RUST_DEPS}
	cargo build

clean:
	cargo clean
