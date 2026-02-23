# botminter Justfile
# Development and documentation tasks for the bm CLI.

# Generator root (where this Justfile lives)
generator_root := justfile_directory()

# Default recipe — list available commands
default:
    @just --list

# Build the bm CLI binary
build:
    cargo build -p bm

# Run all tests
test:
    cargo test -p bm

# Run E2E tests (requires gh auth + podman)
e2e:
    cargo test -p bm --features e2e -- --test-threads=1

# Run E2E tests with output visible
e2e-verbose:
    cargo test -p bm --features e2e -- --test-threads=1 --nocapture

# Run clippy with warnings as errors
clippy:
    cargo clippy -p bm -- -D warnings

# Set up docs virtual environment and install dependencies (idempotent)
docs-setup:
    #!/usr/bin/env bash
    set -euo pipefail
    DOCS_DIR="{{ generator_root }}/docs"
    VENV_DIR="$DOCS_DIR/.venv"
    if [ ! -f "$VENV_DIR/bin/zensical" ]; then
        echo "Setting up docs virtualenv..."
        python3 -m venv "$VENV_DIR"
        "$VENV_DIR/bin/pip" install --quiet -r "$DOCS_DIR/requirements.txt"
        echo "Docs dependencies installed."
    else
        echo "Docs virtualenv already set up."
    fi

# Start live-reload dev server at localhost:8000
docs-serve: docs-setup
    #!/usr/bin/env bash
    set -euo pipefail
    DOCS_DIR="{{ generator_root }}/docs"
    "$DOCS_DIR/.venv/bin/zensical" serve -f "$DOCS_DIR/mkdocs.yml"

# Build static docs site to docs/site/
docs-build: docs-setup
    #!/usr/bin/env bash
    set -euo pipefail
    DOCS_DIR="{{ generator_root }}/docs"
    "$DOCS_DIR/.venv/bin/zensical" build -f "$DOCS_DIR/mkdocs.yml"
    echo "Site built at $DOCS_DIR/site/"
