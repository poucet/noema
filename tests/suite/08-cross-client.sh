# 08-cross-client.sh — Cross-client tool calls (interactive)
# Covers TODO.md Section 8

section "8. Cross-Client Tool Calls"

if $INTERACTIVE; then
    check "From Noema, start chat with tools enabled"
    check "Ask LLM to send message to Discord channel — it uses send_message tool"
    check "Message appears in Discord"
    check "Ask LLM to list channels — list_channels tool works"
else
    skip "Cross-Client Tool Calls" "requires Noema + Discord (use --interactive)"
fi
