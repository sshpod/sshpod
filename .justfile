default:
    @just --list

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

check:
    cargo check --locked

unit-test:
    cargo test --locked

clippy:
    cargo clippy --locked --all-targets --all-features

test: fmt-check check unit-test clippy

deny:
    cargo deny --all-features check

ci: test deny

# Run on a clean, committed tree. Neither command publishes.
package: test
    cargo package --locked
    cargo package --locked --list

release-check: package
    cargo publish --dry-run --locked
