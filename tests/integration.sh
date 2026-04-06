#!/bin/bash
# integration.sh — automated + interactive integration tests for simply-daemon
#
# Usage:
#   tests/integration.sh               # automated REST tests only
#   tests/integration.sh --interactive  # also run guided Discord/MCP tests
#
# Starts an isolated daemon on a random available port with ephemeral storage.
# Tests live in tests/suite/ — .hurl files run via hurl, .sh files are sourced.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
INTERACTIVE=false
[[ "${1:-}" == "--interactive" ]] && INTERACTIVE=true

# ---------------------------------------------------------------------------
# Colors & output
# ---------------------------------------------------------------------------
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
DIM='\033[2m'
NC='\033[0m'

PASSED=0
FAILED=0
SKIPPED=0

pass() { printf "  ${GREEN}PASS${NC}  %s\n" "$1"; ((PASSED++)) || true; }
fail() {
    printf "  ${RED}FAIL${NC}  %s\n" "$1"
    [[ -n "${2:-}" ]] && printf "        ${DIM}%s${NC}\n" "$2"
    ((FAILED++)) || true
}
skip() { printf "  ${YELLOW}SKIP${NC}  %s ${DIM}— %s${NC}\n" "$1" "$2"; ((SKIPPED++)) || true; }
section() { printf "\n${BOLD}══ %s ══${NC}\n" "$1"; }

# ---------------------------------------------------------------------------
# Notes — persisted to tests/results/<timestamp>.md
# ---------------------------------------------------------------------------
RESULTS_DIR="$REPO_ROOT/tests/results"
mkdir -p "$RESULTS_DIR"
NOTES_FILE="$RESULTS_DIR/$(date +%Y-%m-%d-%H%M%S).md"
NOTES_COUNT=0

note() {
    local status="$1" name="$2" text="$3"
    if [[ $NOTES_COUNT -eq 0 ]]; then
        echo "# Integration Test Notes — $(date '+%Y-%m-%d %H:%M')" > "$NOTES_FILE"
        echo "" >> "$NOTES_FILE"
    fi
    printf -- "- **%s** %s: %s\n" "$status" "$name" "$text" >> "$NOTES_FILE"
    ((NOTES_COUNT++)) || true
}

# ---------------------------------------------------------------------------
# Interactive check — one item at a time with optional notes
# ---------------------------------------------------------------------------
check() {
    local name="$1"
    printf "\n  ${DIM}\xe2\x96\xa1${NC} %s\n" "$name"
    printf "    [${GREEN}y${NC}/${RED}n${NC}/${YELLOW}s${NC}kip] "
    read -r answer
    local status
    case "$answer" in
        y|Y) pass "$name"; status="PASS" ;;
        n|N) fail "$name" "manual verification failed"; status="FAIL" ;;
        *)   skip "$name" "skipped by user"; status="SKIP"; return ;;
    esac
    printf "    ${DIM}Notes (enter to skip):${NC} "
    read -r feedback
    if [[ -n "$feedback" ]]; then
        note "$status" "$name" "$feedback"
        printf "    ${DIM}(noted)${NC}\n"
    fi
}

# ---------------------------------------------------------------------------
# Prerequisites
# ---------------------------------------------------------------------------
if ! command -v hurl &>/dev/null; then
    printf "${RED}hurl is required but not installed.${NC}\n"
    printf "Install via: ${BOLD}brew install hurl${NC} or ${BOLD}cargo install hurl${NC}\n"
    exit 1
fi

if ! command -v jq &>/dev/null; then
    printf "${RED}jq is required but not installed.${NC}\n"
    exit 1
fi

# ---------------------------------------------------------------------------
# Isolated test environment
# ---------------------------------------------------------------------------
TEST_DATA_DIR=$(mktemp -d)
DAEMON_PID=""

cleanup() {
    if [[ -n "$DAEMON_PID" ]] && kill -0 "$DAEMON_PID" 2>/dev/null; then
        kill "$DAEMON_PID" 2>/dev/null
        wait "$DAEMON_PID" 2>/dev/null || true
    fi
    rm -rf "$TEST_DATA_DIR"
}
trap cleanup EXIT

# Port 0 = let the OS pick a free port; daemon prints actual port to stdout
mkdir -p "$TEST_DATA_DIR/config"
cat > "$TEST_DATA_DIR/config/settings.toml" <<'EOF'
daemon_port = 0
oauth_callback_port = 0
EOF

# Generate test fixtures
head -c 64 /dev/urandom > "$TEST_DATA_DIR/test.bin"

export NOEMA_DATA_DIR="$TEST_DATA_DIR"
export RUST_LOG="simply_daemon=warn"
export DAEMON_LOG_FILE="$TEST_DATA_DIR/daemon.log"

PORT_FILE="$TEST_DATA_DIR/port"
STDERR_LOG="$TEST_DATA_DIR/stderr.log"

printf "${BOLD}Starting isolated daemon${NC} (data: %s)\n" "$TEST_DATA_DIR"

cd "$REPO_ROOT" && cargo run -p simply-daemon --bin simply-daemon >"$PORT_FILE" 2>"$STDERR_LOG" &
DAEMON_PID=$!

# Wait for daemon to write its port to stdout
printf "Waiting for daemon..."
TEST_PORT=""
for i in $(seq 1 30); do
    if [[ -s "$PORT_FILE" ]]; then
        TEST_PORT=$(head -1 "$PORT_FILE" | tr -d '[:space:]')
        if [[ "$TEST_PORT" =~ ^[0-9]+$ ]]; then
            BASE="http://127.0.0.1:$TEST_PORT"
            printf " ${GREEN}ready on port %s${NC}\n" "$TEST_PORT"
            break
        fi
    fi
    if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
        printf " ${RED}daemon exited${NC}\n"
        echo "Daemon log:"; cat "$TEST_DATA_DIR/daemon.log" 2>/dev/null || true
        echo "Stderr:"; cat "$STDERR_LOG" 2>/dev/null || true
        exit 1
    fi
    printf "."
    sleep 1
done

if [[ -z "$TEST_PORT" ]]; then
    printf " ${RED}timeout${NC}\n"
    echo "Daemon log:"; cat "$TEST_DATA_DIR/daemon.log" 2>/dev/null || true
    echo "Stderr:"; cat "$STDERR_LOG" 2>/dev/null || true
    exit 1
fi

# ---------------------------------------------------------------------------
# Run test suite
# ---------------------------------------------------------------------------
HURL_OK=true

for test_file in "$SCRIPT_DIR"/suite/*; do
    [[ -f "$test_file" ]] || continue
    case "$test_file" in
        *.hurl)
            printf "\n"
            if ! hurl --test --variable "base=$BASE" --file-root "$TEST_DATA_DIR" "$test_file"; then
                HURL_OK=false
            fi
            ;;
        *.sh)
            source "$test_file"
            ;;
    esac
done

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
if [[ $PASSED -gt 0 || $FAILED -gt 0 || $SKIPPED -gt 0 ]]; then
    printf "\n${BOLD}══ Interactive Results ══${NC}\n"
    printf "  ${GREEN}PASS${NC}: %d\n" "$PASSED"
    printf "  ${RED}FAIL${NC}: %d\n" "$FAILED"
    printf "  ${YELLOW}SKIP${NC}: %d\n" "$SKIPPED"
fi

if [[ $NOTES_COUNT -gt 0 ]]; then
    printf "\n  ${DIM}Notes saved to %s${NC}\n" "$NOTES_FILE"
fi

if ! $HURL_OK || [[ $FAILED -gt 0 ]]; then
    exit 1
fi
