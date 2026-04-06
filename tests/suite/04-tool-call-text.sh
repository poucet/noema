# 04-tool-call-text.sh — /tool call text tools (interactive)
# Covers TODO.md Section 4

section "4. /tool call — Text Tools"

if $INTERACTIVE; then
    check "/tool call list_channels — modal opens, returns channel list"
    check "/tool call send_message — message appears in target channel"
    check "/tool call get_channel_history — returns message history"
    check "/tool call search_messages — finds matching messages"
    check "/tool call list_guilds — returns guilds (no params)"
    check "Tools that error show red error embeds"
else
    skip "/tool call — Text Tools" "requires Discord (use --interactive)"
fi
