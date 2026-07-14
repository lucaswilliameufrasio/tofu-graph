.PHONY: fmt lint test build ci audit

fmt:
	cargo fmt --check

lint:
	cargo clippy --all-targets --all-features -- -D warnings

test:
	cargo test --all-targets --all-features

build:
	cargo build --release

audit:
	cargo audit

ci: fmt lint test build
