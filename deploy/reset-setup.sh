#!/usr/bin/env bash
#
# Re-open the first-run setup wizard.
#
# Clears the owner email from settings.toml (which is what marks the daemon as
# "configured"), restarts the container, and prints the fresh /setup link.
# Everything else in your config is left intact — the wizard pre-fills nothing
# secret, so you'll re-enter keys/tokens, but ids/emails persist via the page's
# own draft. A backup of settings.toml is written next to it.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SETTINGS="$HERE/data/config/settings.toml"
URL="$HERE/data/SETUP-URL.txt"

[ -f "$SETTINGS" ] || { echo "No settings.toml at $SETTINGS" >&2; exit 1; }

cp "$SETTINGS" "$SETTINGS.bak"
# Drop the user_email line → daemon boots unconfigured and serves /setup again.
sed -i.tmp '/^[[:space:]]*user_email[[:space:]]*=/d' "$SETTINGS" && rm -f "$SETTINGS.tmp"
rm -f "$URL"

echo "Cleared user_email (backup: $SETTINGS.bak). Restarting…"
( cd "$HERE" && docker compose restart lumina >/dev/null )

printf "Waiting for the new setup link"
for _ in $(seq 1 60); do
	if [ -f "$URL" ]; then
		echo; echo
		sed 's/^/  /' "$URL"
		echo
		exit 0
	fi
	printf "."
	sleep 2
done
echo
echo "No link yet — check: docker compose logs -f lumina"
