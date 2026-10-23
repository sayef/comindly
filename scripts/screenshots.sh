#!/usr/bin/env bash
# Product screenshots from the example company in examples/acme: scripts/screenshots.sh [--no-llm]
# Needs a release build, Node 22+ and Chrome or Edge (or CHROME=/path/to/browser). Never reads your config.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${COMINDLY_BIN:-$ROOT/target/release/comindly}"
OUT="$ROOT/docs/assets/screenshots"
LLM=1
[ "${1:-}" = "--no-llm" ] && LLM=0

# A short, neutral location, because paths show up in the screenshots.
WORK=/tmp/acme
[ -e "$WORK" ] && { echo "$WORK already exists; remove it first" >&2; exit 1; }
mkdir -p "$WORK"
export REAL_HOME="$HOME"
trap '[ -n "${SERVER:-}" ] && kill "$SERVER" 2>/dev/null; rm -rf "$WORK"' EXIT
export HOME="$WORK/home" XDG_CONFIG_HOME="$WORK/home/.config"
unset AWS_PROFILE COMINDLY_AUTHOR_NAME
mkdir -p "$HOME" "$OUT"
INDEX="$WORK/index"

# Each example repository becomes a git repository with one commit by its team.
commit() {
  local repo=$1 team=$2 date=$3
  cp -R "$ROOT/examples/acme/$repo" "$WORK/repos/$repo"
  git -C "$WORK/repos/$repo" init -q -b main
  git -C "$WORK/repos/$repo" add -A
  GIT_AUTHOR_DATE="$date" GIT_COMMITTER_DATE="$date" \
    git -C "$WORK/repos/$repo" -c user.name="$team" -c user.email="${team// /-}@acme.example" \
    commit -q -m "Initial $repo"
}
mkdir -p "$WORK/repos"
commit acme-common "Platform team" "2026-09-01T09:00:00Z"
commit orders "Checkout team" "2026-09-03T10:30:00Z"
commit payments "Payments team" "2026-09-04T14:00:00Z"
commit inventory "Fulfilment team" "2026-09-05T08:15:00Z"
commit storefront "Web team" "2026-09-08T11:45:00Z"
REPOS=("$WORK"/repos/{acme-common,orders,payments,inventory,storefront})

"$BIN" config set index_dir="$INDEX" >/dev/null
for r in "${REPOS[@]}"; do "$BIN" repos add "$r" >/dev/null; done

if [ "$LLM" = 1 ]; then
  "$BIN" link "${REPOS[@]}" --index-dir "$INDEX" --embed --enrich --flows --guide
else
  "$BIN" link "${REPOS[@]}" --index-dir "$INDEX" --embed --no-llm
fi

remember() {
  local who=$1 out; shift
  out=$(COMINDLY_AUTHOR_NAME="$who" "$BIN" remember --index-dir "$INDEX" "$@" 2>&1) || { echo "$out" >&2; return 1; }
  echo "$out" | grep -o '([0-9a-f-]*)' | tr -d '()'
}
idem=$(remember "Payments team" constraint --on PaymentProvider.charge \
  --title "Charges must stay idempotent per order id" \
  --body "SQS delivers at least once, so the worker can see the same order.created twice. The charge reference is derived from the order id; a retry must never create a second charge." \
  --trigger "changing how charges are keyed or retried")
remember "Checkout team" attempt --on InventoryClient.reserve \
  --title "Reserving stock asynchronously over the queue was tried and reverted" \
  --body "Orders were confirmed before stock was held, and a sale oversold 40 items. Reservations stay synchronous in create_order." \
  --field outcome=reverted --evidence "mr:!128" >/dev/null
remember "Platform team" decision --on Money \
  --title "Money is integer cents with a currency, never a float" \
  --body "Rounding differences between services caused refunds of 0.01 EUR. Convert at the edges with Money.from_decimal." >/dev/null
remember "Checkout team" gotcha --on cancel_order \
  --title "Cancelling an order does not release its inventory reservations yet" \
  --trigger "touching cancellation or stock levels" >/dev/null
remember "Web team" question --on Cart.checkout \
  --title "Should checkout check stock again after payment succeeds?" >/dev/null
COMINDLY_AUTHOR_NAME="Checkout team" "$BIN" review "$idem" confirm --index-dir "$INDEX" >/dev/null
"$BIN" conventions add --index-dir "$INDEX" --dimension money "Amounts are acme_common.Money in integer cents; no floats for money" >/dev/null
"$BIN" conventions add --index-dir "$INDEX" --repo payments --dimension queues "Every queue handler is idempotent per message" >/dev/null

PORT=$(( 20000 + RANDOM % 20000 ))
"$BIN" ui --web --port "$PORT" --index-dir "$INDEX" > "$WORK/ui.log" 2>&1 &
SERVER=$!
for _ in $(seq 1 50); do grep -q 'token=' "$WORK/ui.log" && break; sleep 0.2; done
URL=$(grep -o 'http://[^ ]*token=[A-Za-z0-9_-]*' "$WORK/ui.log" | head -1)

node "$ROOT/scripts/screenshots.mjs" "$URL" "$OUT"

# The same index as a team server, for the Members and Your access pages.
kill "$SERVER"; wait "$SERVER" 2>/dev/null || true
for f in admin_token secret_key; do head -c 32 /dev/urandom | base64 | tr -d '/+=\n' > "$WORK/$f"; done
COMINDLY_PUBLIC_URL="http://localhost:$PORT" COMINDLY_ADMIN_EMAILS=lead@acme.example COMINDLY_SIGN_IN=code \
  COMINDLY_ADMIN_TOKEN_FILE="$WORK/admin_token" COMINDLY_SECRET_KEY_FILE="$WORK/secret_key" \
  "$BIN" ui --web --port "$PORT" --index-dir "$INDEX" > "$WORK/server.log" 2>&1 &
SERVER=$!
for _ in $(seq 1 50); do curl -fs "http://localhost:$PORT/health" >/dev/null && break; sleep 0.2; done
SHOT_ADMIN_TOKEN="$(cat "$WORK/admin_token")" node "$ROOT/scripts/screenshots.mjs" "http://localhost:$PORT" "$OUT" server
echo "screenshots in $OUT"
