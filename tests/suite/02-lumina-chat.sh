# 02-lumina-chat.sh — Lumina LLM Chat (interactive)
# Covers TODO.md Section 2

section "2. Lumina LLM Chat"

if $INTERACTIVE; then
    printf "\n  ${BOLD}Prerequisites:${NC} Start Lumina connected to daemon port %s\n" "$TEST_PORT"
    check "Lumina connects to Discord and posts status message"
    check "/chat new creates a chat channel"
    check "Send a message — Lumina responds via LLM"
    check "Response streams with debounced edits"
    check "/chat model <id> changes the model"
    check "/chat pause stops, /chat resume restarts"
    check "Channel history loads as conversation context"
else
    skip "Lumina LLM Chat" "requires Discord (use --interactive)"
fi
