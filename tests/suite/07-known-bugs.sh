# 07-known-bugs.sh — Known bugs verification (interactive)
# Covers TODO.md Section 7

section "7. Known Bugs"

if $INTERACTIVE; then
    check "Snowflake IDs not sent as floats (tool calls succeed with large IDs)"
    check "LLM uses conversation history (not tools) for 'what did I say' questions"
else
    skip "Known Bugs" "requires manual LLM testing (use --interactive)"
fi
