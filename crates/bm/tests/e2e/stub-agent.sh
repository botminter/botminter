#!/bin/bash
# Stub coding agent for E2E testing (CT-04).
# Simulates a coding agent (like Claude Code) that:
# - Records its PID for test verification
# - Records environment variables for test verification
# - Handles SIGINT/SIGTERM with configurable behavior
# - Exits with a configurable exit code after a configurable duration
#
# Environment variables:
#   STUB_AGENT_PID_FILE    — Where to write PID (default: $PWD/.stub-agent-pid)
#   STUB_AGENT_ENV_FILE    — Where to write env (default: $PWD/.stub-agent-env)
#   STUB_AGENT_SIGNAL_LOG  — Where to log signals (default: $PWD/.stub-agent-signals)
#   STUB_AGENT_DURATION    — How long to run in seconds (default: 30)
#   STUB_AGENT_EXIT_CODE   — Exit code to return (default: 0)

PID_FILE="${STUB_AGENT_PID_FILE:-$PWD/.stub-agent-pid}"
ENV_FILE="${STUB_AGENT_ENV_FILE:-$PWD/.stub-agent-env}"
SIGNAL_LOG="${STUB_AGENT_SIGNAL_LOG:-$PWD/.stub-agent-signals}"
DURATION="${STUB_AGENT_DURATION:-30}"
EXIT_CODE="${STUB_AGENT_EXIT_CODE:-0}"

echo $$ > "$PID_FILE"

env | sort > "$ENV_FILE"

trap 'echo "$(date -u +%FT%TZ) SIGINT received" >> "$SIGNAL_LOG"; rm -f "$PID_FILE"; exit 130' INT
trap 'echo "$(date -u +%FT%TZ) SIGTERM received" >> "$SIGNAL_LOG"; rm -f "$PID_FILE"; exit 143' TERM

sleep "$DURATION"

rm -f "$PID_FILE"
exit "$EXIT_CODE"
