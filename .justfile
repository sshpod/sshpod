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

# Opt-in network check: fail when the vendored upstream base schema has changed.
devcontainer-schema-check:
    #!/usr/bin/env bash
    set -euo pipefail
    schema_tmp="$(mktemp)"
    trap 'rm -f -- "$schema_tmp"' EXIT
    curl -fsSL https://raw.githubusercontent.com/devcontainers/spec/main/schemas/devContainer.base.schema.json -o "$schema_tmp"
    if ! cmp -s tests/fixtures/devContainer.base.schema.json "$schema_tmp"; then
        echo "The upstream Dev Container base schema changed; review and update the parser, conformance cases, pinned schema, source metadata, and hash." >&2
        exit 1
    fi
    echo "Vendored Dev Container base schema matches upstream main."

deny:
    cargo deny --all-features check

ci: test deny

# Run on a clean, committed tree. Neither command publishes.
package: test
    cargo package --locked
    cargo package --locked --list

release-check: package
    cargo publish --dry-run --locked
