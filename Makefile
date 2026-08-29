.PHONY: build build-go build-rust

build: build-go build-rust

build-go:
	$(MAKE) -C go build

build-rust:
	cargo build --release --manifest-path rust/Cargo.toml
