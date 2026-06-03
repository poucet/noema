#!/usr/bin/env bash
#
# One command to launch Lumina and get the browser setup link.
#
#   ./start.sh <domain>          e.g.  ./start.sh lumina.simplychris.ai
#
# It seeds public_url, brings up the container, and prints the token-gated
# /setup URL the daemon advertises on first run. Everything else — Discord
# token, Google OAuth, API keys — is entered in that web wizard.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA="$HERE/data"
CONFIG="$DATA/config"
SETTINGS="$CONFIG/settings.toml"
SETUP_URL_FILE="$DATA/SETUP-URL.txt"

DOMAIN="${1:-}"
if [ -z "$DOMAIN" ]; then
	echo "Usage: ./start.sh <domain>     e.g.  ./start.sh lumina.simplychris.ai" >&2
	exit 1
fi
# Tolerate a pasted scheme / trailing path.
DOMAIN="${DOMAIN#https://}"; DOMAIN="${DOMAIN#http://}"; DOMAIN="${DOMAIN%%/*}"

mkdir -p "$CONFIG" "$DATA/vault"

# Seed public_url so the daemon advertises the right setup + OAuth URLs. The
# wizard fills in the rest of settings.toml later (load → mutate → save).
if [ ! -f "$SETTINGS" ]; then
	printf 'public_url = "https://%s"\n' "$DOMAIN" >"$SETTINGS"
	chmod 600 "$SETTINGS"
	echo "Wrote $SETTINGS  (public_url = https://$DOMAIN)"
fi

# Drop any stale link so we only surface the fresh one from this boot.
rm -f "$SETUP_URL_FILE"

echo "Building + starting lumina (first build can take a while)…"
( cd "$HERE" && docker compose up -d --build )

printf "Waiting for the daemon"
for _ in $(seq 1 120); do
	if [ -f "$SETUP_URL_FILE" ]; then
		echo; echo
		echo "  ┌─ Finish setup in your browser ───────────────────────────"
		echo "  │"
		sed 's/^/  │  /' "$SETUP_URL_FILE"
		echo "  │"
		echo "  └─ (this link expires once setup completes) ───────────────"
		exit 0
	fi
	printf "."
	sleep 2
done

echo
echo "No setup link appeared — the daemon is likely already configured, or"
echo "still starting. Check:  docker compose logs -f lumina"
