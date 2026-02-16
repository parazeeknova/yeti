build:
    cargo build

run:
    cargo run

release:
    cargo build --release

install:
    cargo install --path .

fmt:
    cargo fmt

check:
    cargo check

clippy:
    cargo clippy

lint: fmt clippy

test:
    cargo test

clean:
    cargo clean

dev: build run
