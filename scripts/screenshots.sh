#!/usr/bin/env bash
# Product screenshots from the example company in examples/acme: scripts/screenshots.sh [--no-llm]
# Needs Node 22+ and Chrome or Edge (or CHROME=/path/to/browser). Never reads your config.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
if [ -z "${COMINDLY_BIN:-}" ]; then
  (cd "$ROOT/ui" && npm run build)
  cargo build --release -j 4 --manifest-path "$ROOT/Cargo.toml" -p comindly -p comindly-engine
fi
BIN="${COMINDLY_BIN:-$ROOT/target/release/comindly}"
ENGINE="$(dirname "$BIN")/comindly-engine"
OUT="$ROOT/docs/assets/screenshots"
LLM=1
[ "${1:-}" = "--no-llm" ] && LLM=0

# A short, neutral location, because paths show up in the screenshots.
WORK=/tmp/acme
[ -e "$WORK" ] && { echo "$WORK already exists; remove it first" >&2; exit 1; }
mkdir -p "$WORK"
export REAL_HOME="$HOME"
cleanup() {
  [ -n "${SERVER:-}" ] && kill "$SERVER" 2>/dev/null || true
  "$BIN" engine stop >/dev/null 2>&1 || true
  rm -rf "$WORK"
}
trap cleanup EXIT
export HOME="$WORK/home" XDG_CONFIG_HOME="$WORK/home/.config" XDG_DATA_HOME="$WORK/home/.local/share"
unset AWS_PROFILE COMINDLY_AUTHOR_NAME COMINDLY_PROFILE COMINDLY_SERVER COMINDLY_TOKEN COMINDLY_INDEX_DIR
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

# The first command starts the local engine (the one beside $BIN) in this isolated home.
"$BIN" config set index_dir="$INDEX" >/dev/null
for r in "${REPOS[@]}"; do "$BIN" repos add "$r" >/dev/null; done

if [ "$LLM" = 1 ]; then
  "$BIN" link "${REPOS[@]}"
else
  "$BIN" link "${REPOS[@]}" --no-llm
fi

# The engine names records after its `author_name`, so each team sets it before writing.
as() { "$BIN" config set author_name="$1" >/dev/null; }
remember() {
  local who=$1 out; shift
  as "$who"
  out=$("$BIN" remember "$@" 2>&1) || { echo "$out" >&2; return 1; }
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
  --body "cancel_order only publishes order.cancelled; nothing calls the inventory service, so the stock stays held until someone releases it by hand." \
  --trigger "touching cancellation or stock levels" >/dev/null
remember "Web team" question --on Cart.checkout \
  --title "Should checkout check stock again after payment succeeds?" \
  --body "Stock is reserved before the charge; if a reservation expires during a slow payment, the order may confirm items that are gone." >/dev/null
as "Checkout team"
"$BIN" review "$idem" confirm --reason "Charge references are built from the order id in PaymentProvider.charge" >/dev/null
# Work in progress: a discussion on fixing the cancellation gotcha.
did=$("$BIN" discussion open --repo orders --ticket ACME-142 --branch fix/release-stock-on-cancel \
  --title "Release inventory reservations when an order is cancelled" --kind plan \
  --body "Call the inventory service from cancel_order to release what create_order reserved, then publish order.cancelled as before." \
  --on cancel_order --on InventoryClient.reserve 2>&1 | grep -oE '[0-9a-f]{8}-[0-9a-f-]{27}' | head -1)
"$BIN" discussion post "$did" --kind decision --on cancel_order \
  --body "Release before publishing order.cancelled, so payments never refunds an order whose stock is still held. Dropped: releasing from the event consumer, which would leave stock held when the queue is slow." >/dev/null
"$BIN" discussion post "$did" --kind change --evidence "mr:!141" --on cancel_order \
  --body "Review asked to keep cancel_order working when the inventory service is down: a failed release is now retried by the nightly reconciliation instead of failing the cancellation." >/dev/null
as "Web team"
"$BIN" discussion post "$did" --kind question --on Cart.checkout \
  --body "Does the storefront need to refresh stock levels after a cancellation, or is the next page load enough?" >/dev/null
as "Payments team"
"$BIN" discussion open --repo payments --ticket ACME-139 --title "Retry declined charges once with a fresh reference" --kind plan \
  --body "Retry a declined charge once, with a reference derived from the order id and the attempt, so the idempotency constraint still holds." \
  --on PaymentProvider.charge >/dev/null
"$BIN" conventions add --dimension money "Amounts are acme_common.Money in integer cents; no floats for money" >/dev/null
"$BIN" conventions add --repo payments --dimension queues "Every queue handler is idempotent per message" >/dev/null
"$BIN" team set name=Acme >/dev/null

URL=$(node -p 'require(process.argv[1]).url' "$XDG_CONFIG_HOME/comindly/engine.json")
TOKEN=$(tr -d '\n' < "$XDG_CONFIG_HOME/comindly/ui-token")
node "$ROOT/scripts/screenshots.mjs" "$URL/?token=$TOKEN" "$OUT"

# The same index as a team server, for the Members and Your access pages.
"$BIN" engine stop >/dev/null
PORT=$(( 20000 + RANDOM % 20000 ))
for f in admin_token secret_key; do head -c 32 /dev/urandom | base64 | tr -d '/+=\n' > "$WORK/$f"; done
COMINDLY_PUBLIC_URL="http://localhost:$PORT" COMINDLY_ADMIN_EMAILS=lead@acme.example COMINDLY_SIGN_IN=code \
  COMINDLY_ADMIN_TOKEN_FILE="$WORK/admin_token" COMINDLY_SECRET_KEY_FILE="$WORK/secret_key" \
  "$ENGINE" --index-dir "$INDEX" --host 127.0.0.1 --port "$PORT" > "$WORK/server.log" 2>&1 &
SERVER=$!
for _ in $(seq 1 50); do curl -fs "http://localhost:$PORT/health" >/dev/null && break; sleep 0.2; done
SHOT_ADMIN_TOKEN="$(cat "$WORK/admin_token")" node "$ROOT/scripts/screenshots.mjs" "http://localhost:$PORT" "$OUT" server
echo "screenshots in $OUT"
