# 06-mcp-instructions.sh — MCP Instructions channel map (interactive)
# Covers TODO.md Section 6

section "6. MCP Instructions (Channel Map)"

if $INTERACTIVE; then
    check "After Discord ready, MCP instructions contain guild/channel map"
    check "Channel names, IDs, and types listed"
    check "Channels grouped by category"
    check "Create new channel — instructions refresh"
    check "Delete channel — instructions update"
else
    skip "MCP Instructions" "requires Discord (use --interactive)"
fi
