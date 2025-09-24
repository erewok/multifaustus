# just manual: https://github.com/casey/just#readme

_default:
    just --list

# Install cargo plugins used by this project
bootstrap:
    cargo install cargo-nextest
    cargo install cargo-udeps

# Install cargo plugins for building docs
bootstrap-docs:
    cargo install mdbook
    cargo install mdbook-mermaid
    mdbook-mermaid install docs

# Build the project (cargo build)
build *args:
    cargo build {{args}}

# Run code quality checks
check:
    #!/bin/bash -eux
    cargo clippy
    cargo fmt -- --check

# Run code formatting
fmt:
    cargo fmt

# Run all tests locally
test *args:
    cargo nextest run {{args}}

# Build documentation
docs-build:
    mdbook build docs

# Serve documentation
docs-serve:
    mdbook serve docs