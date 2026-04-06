# 05-tool-call-multimodal.sh — /tool call multimodal content (interactive)
# Covers TODO.md Section 5

section "5. /tool call — Multimodal Content"

if $INTERACTIVE; then
    check "Upload image via REST, call get_blob — image returned as Discord attachment"
    check "Audio asset returns as audio file attachment"
    check "BinaryResponse -> RouteMeta.binary_response -> attachment path verified"
else
    skip "/tool call — Multimodal Content" "requires Discord (use --interactive)"
fi
