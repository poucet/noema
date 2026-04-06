# 03-lumina-mcp.sh — Lumina MCP Tools (interactive)
# Covers TODO.md Section 3

section "3. Lumina MCP Tools"

if $INTERACTIVE; then
    printf "\n  ${BOLD}Prerequisites:${NC} Lumina must be connected\n"
    check "Lumina registers as MCP service (check daemon logs)"
    check "/tool list shows tools from daemon + lumina-discord"
    check "Tool descriptions and param counts are correct"
    check "Pagination works if tool list exceeds one page"
else
    skip "Lumina MCP Tools" "requires Lumina (use --interactive)"
fi
