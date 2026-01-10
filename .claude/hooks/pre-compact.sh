#!/bin/bash

# Pre-compact hook for Claude Code
# Reads config and outputs message to Claude

CONFIG_FILE="${CLAUDE_PROJECT_DIR}/.claude/pre-compact-config.md"

if [[ ! -f "$CONFIG_FILE" ]]; then
    echo "ERROR: Config file not found: $CONFIG_FILE" >&2
    exit 2
fi

# Extract phase number from config
PHASE=$(grep -E "^PHASE=" "$CONFIG_FILE" | head -1 | cut -d'=' -f2 | tr -d ' ')

# Extract message (everything after the --- separator)
# and substitute {{PHASE}} with actual phase number
sed -n '/^---$/,$ p' "$CONFIG_FILE" | tail -n +2 | sed "s/{{PHASE}}/${PHASE}/g" >&2

exit 2
