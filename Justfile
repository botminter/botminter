# botminter Justfile
# Development and documentation tasks for the bm CLI.

# Generator root (where this Justfile lives)
generator_root := justfile_directory()

# Default recipe — list available commands
default:
    @just --list

# Build the bm CLI binary (with embedded console assets)
build: console-build
    cargo build -p bm --features console

# Run unit tests only (includes console feature for embedded asset tests)
unit:
    cargo test -p bm --features console

# Run bridge conformance tests only
conformance:
    cargo test -p bm --test conformance

# Run all tests (unit + conformance + e2e)
test: unit conformance e2e

# Run E2E tests ONLY (requires TESTS_GH_TOKEN, TESTS_GH_ORG, and TESTS_APP_* env vars)
e2e:
    @test -n "$TESTS_GH_TOKEN" || { echo "Error: TESTS_GH_TOKEN env var must be set"; exit 1; }
    @test -n "$TESTS_GH_ORG" || { echo "Error: TESTS_GH_ORG env var must be set"; exit 1; }
    @test -n "$TESTS_APP_ID" || { echo "Error: TESTS_APP_ID env var must be set"; exit 1; }
    @test -n "$TESTS_APP_CLIENT_ID" || { echo "Error: TESTS_APP_CLIENT_ID env var must be set"; exit 1; }
    @test -n "$TESTS_APP_INSTALLATION_ID" || { echo "Error: TESTS_APP_INSTALLATION_ID env var must be set"; exit 1; }
    @test -n "$TESTS_APP_PRIVATE_KEY_FILE" || { echo "Error: TESTS_APP_PRIVATE_KEY_FILE env var must be set"; exit 1; }
    cargo test -p bm --features e2e --test e2e -- --gh-token "$TESTS_GH_TOKEN" --gh-org "$TESTS_GH_ORG" --app-id "$TESTS_APP_ID" --app-client-id "$TESTS_APP_CLIENT_ID" --app-installation-id "$TESTS_APP_INSTALLATION_ID" --app-private-key-file "$TESTS_APP_PRIVATE_KEY_FILE" --test-threads=1

# Step through one E2E case at a time (progressive mode). SUITE is optional (e.g., scenario_fresh_start).
e2e-step SUITE="":
    @test -n "$TESTS_GH_TOKEN" || { echo "Error: TESTS_GH_TOKEN env var must be set"; exit 1; }
    @test -n "$TESTS_GH_ORG" || { echo "Error: TESTS_GH_ORG env var must be set"; exit 1; }
    @test -n "$TESTS_APP_ID" || { echo "Error: TESTS_APP_ID env var must be set"; exit 1; }
    @test -n "$TESTS_APP_CLIENT_ID" || { echo "Error: TESTS_APP_CLIENT_ID env var must be set"; exit 1; }
    @test -n "$TESTS_APP_INSTALLATION_ID" || { echo "Error: TESTS_APP_INSTALLATION_ID env var must be set"; exit 1; }
    @test -n "$TESTS_APP_PRIVATE_KEY_FILE" || { echo "Error: TESTS_APP_PRIVATE_KEY_FILE env var must be set"; exit 1; }
    cargo test -p bm --features e2e --test e2e -- --gh-token "$TESTS_GH_TOKEN" --gh-org "$TESTS_GH_ORG" --app-id "$TESTS_APP_ID" --app-client-id "$TESTS_APP_CLIENT_ID" --app-installation-id "$TESTS_APP_INSTALLATION_ID" --app-private-key-file "$TESTS_APP_PRIVATE_KEY_FILE" --progressive {{ SUITE }} --test-threads=1

# Reset progressive E2E state (clean up repos, containers, state files). SUITE is optional.
e2e-reset SUITE="":
    cargo test -p bm --features e2e --test e2e -- --progressive-reset {{ SUITE }}

# Run E2E tests with output visible (note: libtest-mimic does not support --nocapture, but stderr from eprintln! is always visible)
e2e-verbose:
    @test -n "$TESTS_GH_TOKEN" || { echo "Error: TESTS_GH_TOKEN env var must be set"; exit 1; }
    @test -n "$TESTS_GH_ORG" || { echo "Error: TESTS_GH_ORG env var must be set"; exit 1; }
    @test -n "$TESTS_APP_ID" || { echo "Error: TESTS_APP_ID env var must be set"; exit 1; }
    @test -n "$TESTS_APP_CLIENT_ID" || { echo "Error: TESTS_APP_CLIENT_ID env var must be set"; exit 1; }
    @test -n "$TESTS_APP_INSTALLATION_ID" || { echo "Error: TESTS_APP_INSTALLATION_ID env var must be set"; exit 1; }
    @test -n "$TESTS_APP_PRIVATE_KEY_FILE" || { echo "Error: TESTS_APP_PRIVATE_KEY_FILE env var must be set"; exit 1; }
    cargo test -p bm --features e2e --test e2e -- --gh-token "$TESTS_GH_TOKEN" --gh-org "$TESTS_GH_ORG" --app-id "$TESTS_APP_ID" --app-client-id "$TESTS_APP_CLIENT_ID" --app-installation-id "$TESTS_APP_INSTALLATION_ID" --app-private-key-file "$TESTS_APP_PRIVATE_KEY_FILE" --test-threads=1

# Run exploratory tests on bm-test-user@localhost via SSH. Requires SSH access to test user, podman, gh auth.
exploratory-test:
    just -f crates/bm/tests/exploratory/Justfile all

# Run all exploratory tests (alias for exploratory-test)
exploratory-test-full:
    just -f crates/bm/tests/exploratory/Justfile all

# Clean up exploratory test artifacts on remote test user (GitHub repos, containers, keyring)
exploratory-test-clean:
    just -f crates/bm/tests/exploratory/Justfile clean

# Run console (frontend) tests and type checking
console-test:
    cd {{ generator_root }}/console && npm test && npm run check

# Run console dev server (Vite + HMR at localhost:5173)
console-dev:
    cd {{ generator_root }}/console && npm run dev

# Run daemon + console dev server concurrently
dev:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Starting daemon on :8484 and console dev server on :5173..."
    echo "Press Ctrl+C to stop both."
    cd "{{ generator_root }}"
    cargo run -p bm -- daemon start --mode poll --port 8484 &
    DAEMON_PID=$!
    trap "kill $DAEMON_PID 2>/dev/null; wait $DAEMON_PID 2>/dev/null" EXIT
    cd console && npm run dev

# Build console for production (static SPA output to console/build/)
console-build:
    cd {{ generator_root }}/console && npm run build

# Alias for build (kept for backwards compatibility)
build-full: build

# Run clippy with warnings as errors
clippy:
    cargo clippy -p bm --features console -- -D warnings

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

# Tag, create GitHub release with notes, and push
release version notes_file:
    #!/usr/bin/env bash
    set -euo pipefail
    VERSION="{{ version }}"
    NOTES_FILE="{{ notes_file }}"
    if ! echo "$VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+'; then
        echo "Error: version must be semver (e.g., 0.2.0)" >&2
        exit 1
    fi
    if [ ! -f "$NOTES_FILE" ]; then
        echo "Error: notes file '$NOTES_FILE' not found" >&2
        exit 1
    fi
    TAG="v${VERSION}"
    # Update version in Cargo.toml
    sed -i 's/^version = ".*"/version = "'"$VERSION"'"/' crates/bm/Cargo.toml
    # Update Cargo.lock
    cargo generate-lockfile
    # Commit version bump if there are changes
    git add crates/bm/Cargo.toml Cargo.lock
    if ! git diff --cached --quiet; then
        git commit -s -S -m "chore(release): bump version to ${VERSION}"
    fi
    git tag -s -a "$TAG" -m "Release ${TAG}"
    git push origin HEAD "$TAG"
    echo "Pushed ${TAG} — cargo-dist workflow will build, create the release, and attach binaries"
    echo "Once CI completes, set release notes with:"
    echo "  just release-notes ${TAG}"

# Update release notes on an existing GitHub release
release-notes tag notes_file="release-notes.md":
    #!/usr/bin/env bash
    set -euo pipefail
    TAG="{{ tag }}"
    NOTES_FILE="{{ notes_file }}"
    REPO=$(git remote get-url origin | sed -E 's|.*github\.com[:/](.+)\.git$|\1|')
    if [ ! -f "$NOTES_FILE" ]; then
        echo "Error: notes file '$NOTES_FILE' not found" >&2
        exit 1
    fi
    if ! gh release view "$TAG" --repo "$REPO" > /dev/null 2>&1; then
        echo "Error: release '$TAG' not found" >&2
        exit 1
    fi
    gh release edit "$TAG" --repo "$REPO" --notes-file "$NOTES_FILE"
    echo "Release notes updated for ${TAG}"

# Build locally and attach binary to an existing release (fallback if CI fails)
release-build-local tag: console-build
    #!/usr/bin/env bash
    set -euo pipefail
    TAG="{{ tag }}"
    REPO=$(git remote get-url origin | sed -E 's|.*github\.com[:/](.+)\.git$|\1|')
    # Verify the release exists
    if ! gh release view "$TAG" --repo "$REPO" > /dev/null 2>&1; then
        echo "Error: release '$TAG' not found" >&2
        exit 1
    fi
    TARGET=$(rustc -vV | grep '^host:' | awk '{print $2}')
    echo "Building for ${TARGET}..."
    cargo build --release -p bm --target "$TARGET"
    cd "target/${TARGET}/release"
    tar czf "../../../bm-${TARGET}.tar.gz" bm
    cd ../../..
    gh release upload "$TAG" "bm-${TARGET}.tar.gz" --repo "$REPO"
    rm "bm-${TARGET}.tar.gz"
    echo "Attached bm-${TARGET}.tar.gz to ${TAG}"
