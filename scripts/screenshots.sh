#!/usr/bin/env bash
# Product screenshots from the example clinical-trials platform in examples/trials: scripts/screenshots.sh [--no-llm]
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
WORK=/tmp/northvile
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
  cp -R "$ROOT/examples/trials/$repo" "$WORK/repos/$repo"
  git -C "$WORK/repos/$repo" init -q -b main
  git -C "$WORK/repos/$repo" add -A
  GIT_AUTHOR_DATE="$date" GIT_COMMITTER_DATE="$date" \
    git -C "$WORK/repos/$repo" -c user.name="$team" -c user.email="${team// /-}@northvile.example" \
    commit -q -m "Initial $repo"
}
mkdir -p "$WORK/repos"
commit northvile-common "Platform team" "2026-09-01T09:00:00Z"
commit northvile-jvm-common "Platform team" "2026-09-02T10:15:00Z"
commit audit-trail "Platform team" "2026-09-03T08:40:00Z"
commit trial-registry "Study Build team" "2026-09-07T11:20:00Z"
commit randomisation "Subject Management team" "2026-09-09T14:05:00Z"
commit enrolment "Subject Management team" "2026-09-10T09:30:00Z"
commit consent "Subject Management team" "2026-09-14T13:10:00Z"
commit visit-scheduler "Site Experience team" "2026-09-16T10:00:00Z"
commit lab-ingest "Clinical Data team" "2026-09-17T08:20:00Z"
commit site-portal "Site Experience team" "2026-09-21T15:45:00Z"
commit edc "Clinical Data team" "2026-09-23T12:00:00Z"
commit safety "Pharmacovigilance team" "2026-09-24T09:50:00Z"
REPOS=("$WORK"/repos/*)

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
clock=$(remember "Pharmacovigilance team" constraint --on susar_due \
  --title "SUSAR clocks start at sponsor awareness, in UTC, and never restart" \
  --body "Fatal or life-threatening SUSARs are due 7 days after the sponsor first became aware, all others 15 days. Follow-up information and late causality assessments do not reset the clock; aware_at is stored once and never updated." \
  --trigger "changing how deadlines, awareness dates or follow-ups are handled")
remember "Subject Management team" attempt --on RandomisationClient.randomise \
  --title "Returning the arm to enrolment for kit lookup was tried and reverted" \
  --body "The arm ended up in enrolment's request logs and in a support export that site monitors could read. Randomisation now returns only the kit code; enrolment rejects any response that carries an arm." \
  --field outcome=reverted --evidence "mr:!214" >/dev/null
remember "Platform team" decision --on record_audit_event \
  --title "Python services audit through the audit-events topic; JVM and Go services call audit-trail over HTTP" \
  --body "The Python services already publish domain events through northvile-common, so audit rides the same path and survives an audit-trail outage. JVM and Go services write synchronously so a change and its audit entry fail together." >/dev/null
remember "Pharmacovigilance team" gotcha --on candidate_from_flag \
  --title "The lab-flag consumer reads enrolment's subjects table directly" \
  --body "candidate_from_flag looks up the site with raw SQL on subjects, which enrolment owns. A rename or a new status in enrolment's schema breaks lab-flag processing silently; the messages go back to the queue until they expire." \
  --trigger "changing the subjects table or how safety finds a subject's site" >/dev/null
remember "Site Experience team" question --on ScreeningWizard.sign \
  --title "What should the portal do when the protocol is amended between consent and enrolment?" \
  --body "The wizard stops at signing if the form's protocol version differs from the screening's, but a signature made the day before an amendment passes, and enrolment then refuses it with a 409." >/dev/null
remember "Clinical Data team" constraint --on BlindingPolicy.requireNoArm \
  --title "No treatment arm in CRF values or audit events" \
  --body "Monitors, site staff and data managers are blinded and read both. Unblinded data stays in randomisation and in safety's report documents; requireNoArm guards every write from the JVM services." >/dev/null
as "Study Build team"
"$BIN" review "$clock" confirm --reason "Deadlines are computed from aware_at in susar_due; add_follow_up never touches it" >/dev/null
# Work in progress: re-planning visits after an amendment.
as "Site Experience team"
did=$("$BIN" discussion open --repo visit-scheduler --ticket NVC-412 --branch fix/replan-after-reconsent \
  --title "Re-plan visits only once a subject has re-consented to the amended protocol" --kind plan \
  --body "reschedule_study re-plans every enrolled subject as soon as protocol.amended arrives. Re-plan per subject instead, when consent records the new signature, and keep the old schedule until then." \
  --on reschedule_study --on handle_protocol_change 2>&1 | grep -oE '[0-9a-f]{8}-[0-9a-f-]{27}' | head -1)
"$BIN" discussion post "$did" --kind decision --on handle_protocol_change \
  --body "Keep consuming visit-protocol-changes, but only record the pending version; the re-plan is triggered per subject. Dropped: polling consent, which would put a schedule change behind a 15-minute sweep." >/dev/null
"$BIN" discussion post "$did" --kind change --evidence "mr:!418" --on reschedule_study \
  --body "Review asked to keep completed and in-window visits untouched: only planned visits after the re-consent date are re-planned." >/dev/null
as "Subject Management team"
"$BIN" discussion post "$did" --kind question --on ReconsentService.flagSubjects \
  --body "Should consent publish a reconsent.signed event for this, or is the audit entry enough to drive it?" >/dev/null
as "Pharmacovigilance team"
"$BIN" discussion open --repo safety --ticket NVC-431 --title "Find a subject's site through enrolment's API instead of its table" --kind plan \
  --body "Replace the raw SQL on subjects in candidate_from_flag with SubjectClient.get, and dead-letter flags for unknown subjects instead of retrying them forever." \
  --on candidate_from_flag --on SubjectClient.get >/dev/null
"$BIN" conventions add --dimension time "Timestamps are UTC (utc_now, UtcClock); trial dates are UTC calendar dates, never site-local" >/dev/null
"$BIN" conventions add --repo safety --dimension queues "Every queue consumer is idempotent per event_id" >/dev/null
"$BIN" team set name="Northvile Clinical" >/dev/null

URL=$(node -p 'require(process.argv[1]).url' "$XDG_CONFIG_HOME/comindly/engine.json")
TOKEN=$(tr -d '\n' < "$XDG_CONFIG_HOME/comindly/ui-token")
node "$ROOT/scripts/screenshots.mjs" "$URL/?token=$TOKEN" "$OUT"

# The same index as a team server, for the Members and Your access pages.
"$BIN" engine stop >/dev/null
PORT=$(( 20000 + RANDOM % 20000 ))
for f in admin_token secret_key; do head -c 32 /dev/urandom | base64 | tr -d '/+=\n' > "$WORK/$f"; done
COMINDLY_PUBLIC_URL="http://localhost:$PORT" COMINDLY_ADMIN_EMAILS=lead@northvile.example COMINDLY_SIGN_IN=code \
  COMINDLY_ADMIN_TOKEN_FILE="$WORK/admin_token" COMINDLY_SECRET_KEY_FILE="$WORK/secret_key" \
  "$ENGINE" --index-dir "$INDEX" --host 127.0.0.1 --port "$PORT" > "$WORK/server.log" 2>&1 &
SERVER=$!
for _ in $(seq 1 50); do curl -fs "http://localhost:$PORT/health" >/dev/null && break; sleep 0.2; done
SHOT_ADMIN_TOKEN="$(cat "$WORK/admin_token")" node "$ROOT/scripts/screenshots.mjs" "http://localhost:$PORT" "$OUT" server
echo "screenshots in $OUT"
