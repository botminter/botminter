#!/bin/bash
# Stub ralph binary for E2E testing.
# Simulates ralph's basic behavior without making any API calls.
#
# Supports SIGTERM ignore mode: if .ralph-stub-ignore-sigterm exists in $PWD,
# SIGTERM is trapped and logged to .ralph-stub-sigterm.log instead of exiting.

case "$1" in
  run)
    echo $$ > "$PWD/.ralph-stub-pid"
    if [ -n "$RALPH_TELEGRAM_API_URL" ] && [ -n "$RALPH_TELEGRAM_BOT_TOKEN" ]; then
      curl -s "${RALPH_TELEGRAM_API_URL}/bot${RALPH_TELEGRAM_BOT_TOKEN}/getUpdates" \
        > "$PWD/.ralph-stub-tg-response" 2>&1
    fi
    if [ -n "$RALPH_MATRIX_ACCESS_TOKEN" ] && [ -n "$RALPH_MATRIX_HOMESERVER_URL" ]; then
      curl -s "${RALPH_MATRIX_HOMESERVER_URL}/_matrix/client/versions" \
        > "$PWD/.ralph-stub-matrix-response" 2>&1
    fi
    env | grep -E '^(RALPH_|GH_TOKEN|GH_CONFIG_DIR)' | sort > "$PWD/.ralph-stub-env"
    # Install SIGTERM trap BEFORE the polling window so it is active from the very start.
    # Dynamic check at signal time: if .ralph-stub-ignore-sigterm exists, ignore; else exit.
    # This prevents the default SIGTERM handler from killing the script during the 5-second
    # startup poll (race window where the trap was previously not yet installed).
    trap '
      if [ -f "$PWD/.ralph-stub-ignore-sigterm" ]; then
        echo "$(date -u +%FT%TZ) SIGTERM received and ignored" >> "$PWD/.ralph-stub-sigterm.log"
      else
        rm -f "$PWD/.ralph-stub-pid"
        exit 0
      fi
    ' SIGTERM
    trap "rm -f \"$PWD/.ralph-stub-pid\"; exit 0" SIGINT EXIT
    # Poll for up to 5 seconds so tests can write .ralph-stub-ignore-sigterm after workspace
    # creation but before stub-ralph enters the main loop.
    _si=0
    while [ $_si -lt 50 ] && [ ! -f "$PWD/.ralph-stub-ignore-sigterm" ]; do
        sleep 0.1
        _si=$((_si + 1))
    done
    if [ -f "$PWD/.ralph-stub-ignore-sigterm" ]; then
      echo "$(date -u +%FT%TZ) SIGTERM trap active, ignore-file present" >> "$PWD/.ralph-stub-sigterm.log"
    fi
    while true; do sleep 1; done
    ;;
  loops)
    if [ "$2" = "stop" ]; then
      pid_file="$PWD/.ralph-stub-pid"
      if [ -f "$pid_file" ]; then
        kill "$(cat "$pid_file")" 2>/dev/null
        rm -f "$pid_file"
      fi
      exit 0
    fi
    ;;
  *)
    exit 0
    ;;
esac
