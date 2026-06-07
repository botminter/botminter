#!/usr/bin/env bash
# Shared helpers for exploratory tests.
# Sourced by each phase script via: source "$LIB"
#
# Provides:
#   - Reporting: pass(), fail(), note(), header()
#   - Keyring isolation: ensure_keyring(), stop_isolated_keyring()
#   - Command wrappers: bm(), bm_agent(), secret_tool()

# ── Reporting ──────────────────────────────────────────────────

pass() {
    echo "| $1 | $2 | **PASS** |" >> "$REPORT"
    echo "  ✓ $1: $2"
}

fail() {
    echo "| $1 | $2 | **FAIL** — $3 |" >> "$REPORT"
    echo "  ✗ $1: $2 — $3"
}

note() {
    echo "| $1 | $2 | **NOTE** — $3 |" >> "$REPORT"
    echo "  ℹ $1: $2 — $3"
}

header() {
    echo "" >> "$REPORT"
    echo "### $1" >> "$REPORT"
    echo "" >> "$REPORT"
    echo "| # | Test | Result |" >> "$REPORT"
    echo "|---|------|--------|" >> "$REPORT"
    echo "$1"
    echo "$(echo "$1" | sed 's/.$/=/g; s/./=/g')"
}

# ── Isolated D-Bus + Keyring ──────────────────────────────────
# Mirrors the E2E TestEnv pattern (test_env.rs:143-272).
# Each exploratory test run gets its own dbus-daemon + gnome-keyring-daemon
# so tests don't depend on (or pollute) the system keyring.

DBUS_STATE_FILE="/tmp/bm-exploratory-dbus.env"

start_isolated_keyring() {
    # Check for existing (possibly stale) state
    if [ -f "$DBUS_STATE_FILE" ]; then
        # shellcheck source=/dev/null
        source "$DBUS_STATE_FILE"
        if kill -0 "$ISOLATED_DBUS_PID" 2>/dev/null; then
            echo "  Reusing existing isolated keyring (pid=$ISOLATED_DBUS_PID)"
            export ISOLATED_DBUS_ADDR ISOLATED_DBUS_PID ISOLATED_DBUS_TMPDIR
            return 0
        fi
        # Stale — clean up
        rm -rf "$ISOLATED_DBUS_TMPDIR" 2>/dev/null || true
        rm -f "$DBUS_STATE_FILE"
    fi

    local tmpdir
    tmpdir=$(mktemp -d /tmp/bm-exploratory-dbus.XXXXXX)
    mkdir -p "$tmpdir/runtime" "$tmpdir/data"

    # Start isolated dbus-daemon (same flags as test_env.rs:151-155)
    local dbus_out dbus_addr dbus_pid
    dbus_out=$(XDG_RUNTIME_DIR="$tmpdir/runtime" \
        dbus-daemon --session --fork --print-address --print-pid 2>&1)
    dbus_addr=$(echo "$dbus_out" | head -1)
    dbus_pid=$(echo "$dbus_out" | tail -1)

    if [ -z "$dbus_addr" ] || [ -z "$dbus_pid" ]; then
        echo "FATAL: dbus-daemon failed to start. Output: $dbus_out"
        rm -rf "$tmpdir"
        exit 1
    fi

    # Start gnome-keyring-daemon on isolated bus (same as test_env.rs:229-248)
    # Empty password via stdin pipe, XDG vars for isolation
    echo "" | DBUS_SESSION_BUS_ADDRESS="$dbus_addr" \
        XDG_RUNTIME_DIR="$tmpdir/runtime" \
        XDG_DATA_HOME="$tmpdir/data" \
        gnome-keyring-daemon --replace --unlock \
            --components=secrets,pkcs11 --daemonize >/dev/null 2>&1
    sleep 1

    # Verify unlock (same as test_env.rs:251-272)
    local locked
    locked=$(DBUS_SESSION_BUS_ADDRESS="$dbus_addr" \
        busctl --user get-property org.freedesktop.secrets \
            /org/freedesktop/secrets/collection/login \
            org.freedesktop.Secret.Collection Locked 2>/dev/null || echo "b true")
    if echo "$locked" | grep -q "b false"; then
        echo "  ✓ Isolated keyring unlocked (dbus pid=$dbus_pid)"
    else
        echo "FATAL: Isolated keyring failed to unlock: $locked"
        kill "$dbus_pid" 2>/dev/null || true
        rm -rf "$tmpdir"
        exit 1
    fi

    # Persist state for cross-phase use (each phase is a separate process)
    cat > "$DBUS_STATE_FILE" <<EOF
ISOLATED_DBUS_ADDR=$dbus_addr
ISOLATED_DBUS_PID=$dbus_pid
ISOLATED_DBUS_TMPDIR=$tmpdir
EOF

    export ISOLATED_DBUS_ADDR="$dbus_addr"
    export ISOLATED_DBUS_PID="$dbus_pid"
    export ISOLATED_DBUS_TMPDIR="$tmpdir"
}

load_isolated_keyring() {
    if [ ! -f "$DBUS_STATE_FILE" ]; then
        return 1
    fi
    # shellcheck source=/dev/null
    source "$DBUS_STATE_FILE"
    # Verify PID is alive (handles stale state from crashes)
    if ! kill -0 "$ISOLATED_DBUS_PID" 2>/dev/null; then
        rm -rf "$ISOLATED_DBUS_TMPDIR" 2>/dev/null || true
        rm -f "$DBUS_STATE_FILE"
        return 1
    fi
    export ISOLATED_DBUS_ADDR ISOLATED_DBUS_PID ISOLATED_DBUS_TMPDIR
}

ensure_keyring() {
    if load_isolated_keyring; then
        return 0
    fi
    start_isolated_keyring
}

stop_isolated_keyring() {
    if [ -f "$DBUS_STATE_FILE" ]; then
        # shellcheck source=/dev/null
        source "$DBUS_STATE_FILE"
        kill "$ISOLATED_DBUS_PID" 2>/dev/null || true
        rm -rf "$ISOLATED_DBUS_TMPDIR" 2>/dev/null || true
        rm -f "$DBUS_STATE_FILE"
    fi
}

# ── Command Wrappers ──────────────────────────────────────────
# bm() routes keyring ops to the isolated D-Bus via BM_KEYRING_DBUS.
# Podman subprocesses keep the real system DBUS_SESSION_BUS_ADDRESS.
# See credential.rs:103-119 for the BM_KEYRING_DBUS mechanism.

bm() {
    BM_KEYRING_DBUS="${ISOLATED_DBUS_ADDR:-}" "$BM" "$@"
}

bm_agent() {
    "$BM_AGENT" "$@"
}

# secret-tool doesn't know about BM_KEYRING_DBUS, so we set
# DBUS_SESSION_BUS_ADDRESS per-command (never exported globally).
secret_tool() {
    DBUS_SESSION_BUS_ADDRESS="${ISOLATED_DBUS_ADDR:-}" secret-tool "$@"
}

# ── GH_TOKEN ──────────────────────────────────────────────────

ensure_gh_token() {
    if [ -z "${GH_TOKEN:-}" ]; then
        export GH_TOKEN
        GH_TOKEN=$(gh auth token)
    fi
}

# ── Hire Wrapper ──────────────────────────────────────────────
# bm_hire wraps `bm hire` with --reuse-app and pre-provisioned App credentials.
# Usage: bm_hire <role> --name <name> [extra args...]
# Requires TESTS_APP_ID, TESTS_APP_CLIENT_ID, TESTS_APP_INSTALLATION_ID,
# TESTS_APP_PRIVATE_KEY_FILE env vars.

bm_hire() {
    bm hire "$@" \
        -t "$TEAM" \
        --reuse-app \
        --app-id "$TESTS_APP_ID" \
        --client-id "$TESTS_APP_CLIENT_ID" \
        --private-key-file "$TESTS_APP_PRIVATE_KEY_FILE" \
        --installation-id "$TESTS_APP_INSTALLATION_ID"
}
