.PHONY: build install

build:
	cargo build --release

install:
	scripts/install.sh
