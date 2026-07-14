#!/usr/bin/env bash
# Fetch the Stage 1 real sample: 4 ETFs over one real trading week.
#
# This is a THROWAWAY SCRIPT, not application code. It exists so the pipeline
# itself never needs a network stack: no HTTP dependency in Cargo.toml, no API
# keys read by the binary, no rate-limit or retry logic to get wrong. It runs
# once, writes raw vendor payloads to disk, and you commit them. From then on
# the sample is reproducible forever and cannot rot when a vendor changes an
# endpoint.
#
# Live, rate-limit-aware fetching is Stage 2's job.
#
# USAGE
#   cp .env.example .env    # then fill in your keys
#   ./scripts/fetch_sample.sh
#
# ...or export the vars yourself; already-exported values win over .env.
#
# Then eyeball the payloads (that inspection is the entire point of Stage 1),
# commit them, and run:
#   cargo run -- run --config configs/stage1_saved_sample.json

set -euo pipefail

# Run from the repo root regardless of where the script was invoked from, so
# the relative output path below always lands in the right place.
cd "$(dirname "${BASH_SOURCE[0]}")/.."

# Load .env if present. Existing environment variables take precedence, so an
# explicit `export` in your shell always beats the file. `set -a` exports every
# assignment; the subshell-free sourcing keeps it simple, and .env is gitignored.
if [[ -f .env ]]; then
  echo "==> Loading .env"
  while IFS='=' read -r key value; do
    # Skip blanks and comments.
    [[ -z "$key" || "$key" == \#* ]] && continue
    # Do not overwrite a variable already set in the environment.
    [[ -n "${!key:-}" ]] && continue
    # Strip surrounding quotes if present.
    value="${value%\"}"; value="${value#\"}"
    value="${value%\'}"; value="${value#\'}"
    [[ -n "$value" ]] && export "$key=$value"
  done < .env
fi

OUT_DIR="fixtures/saved_sample"

# One real trading week. It MUST contain a weekend boundary and a real holiday
# or early close — that is what forces calendar.rs (design.md Decision 2, the
# DST/early-close logic) to meet real dates for the first time.
#
# 2025-07-01 .. 2025-07-07 spans:
#   - Thu 2025-07-03: NYSE EARLY CLOSE (13:00 ET, day before Independence Day)
#   - Fri 2025-07-04: NYSE HOLIDAY (Independence Day)
#   - Sat/Sun 05-06:  weekend boundary
# Change these if you want a different week, but keep a holiday/early close.
START="2025-07-01"
END="2025-07-07"

# The 4 ETFs are the spec's core universe. The 8 large-caps are here because the
# source probe (2026-07-14) found Massive tags news to COMPANIES, not ETFs: of
# 677 articles in this week, only 4 mentioned SPY/QQQ/DIA/IWM, while AMZN alone
# had 57. Without the large-caps there is essentially no finance news to test, and
# `constituent_etf_map` in the config propagates their stories back to QQQ/SPY.
#
# Price bars are fetched for ALL 12 — a symbol with news but no bars produces no
# observations at all, which would defeat the point of adding it.
SYMBOLS=(SPY QQQ DIA IWM AAPL AMZN AMD AVGO GOOGL MSFT NVDA TSLA)

mkdir -p "$OUT_DIR"

require_env() {
  if [[ -z "${!1:-}" ]]; then
    echo "error: \$$1 is not set. See the usage comment at the top of this script." >&2
    exit 1
  fi
}

echo "==> Fetching Massive news (finance) ..."
require_env MASSIVE_API_KEY

# Massive's free tier rate-limits aggressively (429). Back off and retry rather
# than failing the whole fetch — a half-fetched sample is worse than a slow one,
# because a missing symbol looks exactly like a symbol with no news.
#
# The Bearer header is confirmed working (probed 2026-07-14); the ?apiKey= query
# param also works. We use Bearer and do NOT silently fall back, because a
# fallback masked a 429 as an auth failure and sent us chasing the wrong bug.
massive_fetch() {
  local symbol="$1" out="$2" attempt=1 code
  while (( attempt <= 5 )); do
    code=$(curl -s -o "$out" -w '%{http_code}' -G \
      -H "Authorization: Bearer ${MASSIVE_API_KEY}" \
      "https://api.massive.com/v2/reference/news" \
      --data-urlencode "ticker=${symbol}" \
      --data-urlencode "published_utc.gte=${START}" \
      --data-urlencode "published_utc.lte=${END}" \
      --data-urlencode "limit=1000" \
      --data-urlencode "order=asc")
    case "$code" in
      200) return 0 ;;
      429)
        local wait=$(( attempt * 20 ))
        echo "    ${symbol}: rate limited (429), waiting ${wait}s (attempt ${attempt}/5)" >&2
        sleep "$wait"
        (( attempt++ ))
        ;;
      401|403)
        echo "    ${symbol}: HTTP ${code} — MASSIVE_API_KEY rejected. Check .env." >&2
        return 1
        ;;
      *)
        echo "    ${symbol}: HTTP ${code}" >&2
        cat "$out" >&2; echo >&2
        return 1
        ;;
    esac
  done
  echo "    ${symbol}: still rate limited after 5 attempts. Wait a minute and re-run." >&2
  return 1
}

for symbol in "${SYMBOLS[@]}"; do
  echo "    $symbol"
  massive_fetch "$symbol" "${OUT_DIR}/massive_${symbol}.json"
  # Free tier is ~5 requests/minute. Pace deliberately rather than hammering and
  # backing off; 13s keeps us under the limit on the first pass.
  sleep 13
done

echo "==> Fetching GDELT broad news (no API key required) ..."
# DOC 2.0 wants YYYYMMDDHHMMSS. Broad macro query; deliberately not
# ticker-scoped, because GDELT's job here is macro/policy/rates coverage.
GDELT_START="${START//-/}000000"
GDELT_END="${END//-/}235959"
# GDELT rate-limits too (429), despite needing no key. Back off and retry.
gdelt_attempt=1
while (( gdelt_attempt <= 5 )); do
  gdelt_code=$(curl -s -o "${OUT_DIR}/gdelt_macro.json" -w '%{http_code}' -G \
    "https://api.gdeltproject.org/api/v2/doc/doc" \
    --data-urlencode 'query=(stocks OR "federal reserve" OR inflation OR "interest rates") sourcelang:english' \
    --data-urlencode "startdatetime=${GDELT_START}" \
    --data-urlencode "enddatetime=${GDELT_END}" \
    --data-urlencode "mode=ArtList" \
    --data-urlencode "maxrecords=250" \
    --data-urlencode "format=json")
  if [[ "$gdelt_code" == "200" ]]; then
    break
  fi
  gdelt_wait=$(( gdelt_attempt * 15 ))
  echo "    GDELT: HTTP ${gdelt_code}, waiting ${gdelt_wait}s (attempt ${gdelt_attempt}/5)" >&2
  sleep "$gdelt_wait"
  (( gdelt_attempt++ ))
done
if [[ "$gdelt_code" != "200" ]]; then
  echo "error: GDELT fetch failed with HTTP ${gdelt_code} after 5 attempts." >&2
  exit 1
fi

echo "==> Fetching Alpaca hourly bars ..."
require_env APCA_API_KEY_ID
require_env APCA_API_SECRET_KEY
# NOTE: this payload answers open question S1-A. Look at the "t" values: are the
# bars session-aligned (:30, i.e. 09:30 ET) or clock-aligned (:00)? The answer
# decides whether build_observations' contiguous-coverage rule silently drops
# most of the sample. Do not assume — read them.
#
# Alpaca PAGINATES: it caps a response and hands back `next_page_token`. Ignoring
# that cursor silently truncated our first fetch to 8 of 12 symbols — SPY, QQQ,
# NVDA and TSLA vanished entirely. A missing symbol is indistinguishable from a
# symbol with no data, which is precisely the silent-drop failure the spec
# forbids. So: follow the cursor until it is exhausted, and merge the pages.
page=1
token=""
: > /tmp/alpaca_pages.jsonl
while :; do
  args=(-G
    -H "APCA-API-KEY-ID: ${APCA_API_KEY_ID}"
    -H "APCA-API-SECRET-KEY: ${APCA_API_SECRET_KEY}"
    --data-urlencode "symbols=$(IFS=,; echo "${SYMBOLS[*]}")"
    --data-urlencode "timeframe=1Hour"
    --data-urlencode "start=${START}T00:00:00Z"
    --data-urlencode "end=${END}T23:59:59Z"
    --data-urlencode "limit=10000"
    --data-urlencode "adjustment=all"
    --data-urlencode "feed=iex")
  [[ -n "$token" ]] && args+=(--data-urlencode "page_token=${token}")

  curl -fsS "${args[@]}" "https://data.alpaca.markets/v2/stocks/bars" -o /tmp/alpaca_page.json
  cat /tmp/alpaca_page.json >> /tmp/alpaca_pages.jsonl
  echo >> /tmp/alpaca_pages.jsonl

  token=$(jq -r '.next_page_token // empty' /tmp/alpaca_page.json)
  echo "    page ${page}: $(jq '[.bars[]?[]]|length' /tmp/alpaca_page.json) bars$([[ -n "$token" ]] && echo ", more to fetch")"
  [[ -z "$token" ]] && break
  (( page++ ))
done

# Merge every page's `bars` map into one, concatenating each symbol's arrays.
jq -s '{bars: (reduce .[].bars as $b ({}; reduce ($b|keys_unsorted[]) as $k (.; .[$k] = ((.[$k] // []) + $b[$k]))))}' \
  /tmp/alpaca_pages.jsonl > "${OUT_DIR}/alpaca_bars.json"

echo "    merged: $(jq '[.bars[][]]|length' "${OUT_DIR}/alpaca_bars.json") bars across $(jq '.bars|length' "${OUT_DIR}/alpaca_bars.json") symbols"

# Every requested symbol must be present. A silently missing symbol would look
# exactly like a symbol nobody wrote news about.
missing=$(jq -r --arg want "$(IFS=,; echo "${SYMBOLS[*]}")" \
  '($want|split(",")) - (.bars|keys) | join(", ")' "${OUT_DIR}/alpaca_bars.json")
if [[ -n "$missing" ]]; then
  echo "error: Alpaca returned no bars for: ${missing}" >&2
  echo "       Refusing to write a partial sample." >&2
  exit 1
fi

echo
echo "==> Done. Payloads in ${OUT_DIR}:"
ls -la "${OUT_DIR}"
echo
echo "Next:"
echo "  1. LOOK at the payloads. Stage 1 is a timestamp-and-leakage inspection;"
echo "     the reading is the deliverable, not the running."
echo "     - Alpaca bar 't' minute values -> answers S1-A (bar alignment)."
echo "     - Massive 'published_utc' -> any blank/malformed? those get quarantined."
echo "     - Repeated titles across different URLs -> answers S1-B (syndication)."
echo "  2. Record what you found in fixtures/saved_sample/README.md."
echo "  3. git add fixtures/saved_sample/ && commit."
echo "  4. cargo run -- run --config configs/stage1_saved_sample.json"
