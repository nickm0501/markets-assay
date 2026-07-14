# Stage 2 Implementation Plan — Make the Pipeline Capable of the Real Experiment

> **Living design doc:** `docs/superpowers/design/correlation-first-pipeline/design.md`.
> Decisions 1-21 govern this plan. The bake-off that chose the scorer is
> `docs/superpowers/design/correlation-first-pipeline/2026-07-14-sentiment-scorer-bakeoff.md`.

**Goal:** Close the three things that stand between us and a real two-year hypothesis test: we cannot *read* the news, we cannot tell *signal from noise*, and we cannot *fetch two years of data*.

## The reframe

The spec calls Stage 2 a *"narrow Massive/GDELT/Alpaca source probe."* **Stage 1 already did the probing** — we know how all three sources behave, what they tag, what they omit, how they paginate, and how they rate-limit. That work is done and its findings are in `fixtures/saved_sample/README.md`.

So Stage 2's real content is what the probe *revealed we still need*:

| # | gap | why it blocks Stage 3 |
|---|---|---|
| **A** | **The scorer cannot read the news.** `lexicon_hit_rate = 0.2021`. | Every downstream number is meaningless. We are measuring silence, not sentiment. |
| **B** | **Only 1 of the spec's 4 baselines exists.** | Without them we cannot distinguish a real edge from noise — which is the entire point. This is Decision 6's still-open half. |
| **C** | **Ingestion cannot survive a two-year fetch.** No live source, no rate limiting, no resume. | Massive's free tier is ~5 req/min. Two years × 29 symbols is a **multi-hour** job that *will* be interrupted. The spec requires ingestion be "idempotent and resumable"; it is neither. |

---

## Scope

### In scope

- Ship the LM+VADER scorer chosen by the bake-off (Decision 20).
- Store vendor sentiment as a benchmark; **never** trade on it (Decision 21).
- Build the 3 missing baselines and the seed policy they need.
- A live-API source behind the *existing* `NewsSource`/`PriceSource` trait, with rate limiting, retry/backoff, and pagination.
- Idempotent, resumable ingestion with a raw-payload cache.
- The spec's unimplemented Data Quality requirements (see Task 7).

### Explicitly NOT in scope

- **The full First Experiment Matrix.** The spec wants price intervals of 1h *and* 15m; news windows including *session-to-now*; horizons of next-hour, next-4h, *same-day close*, and *next regular session*; thresholds at both 10% and 20%; and targets beyond return (direction, volatility expansion, tail events). We have **2 of ~3 windows, 1 of 4 horizons, 1 of 2 threshold pairs, 1 of 4 targets.** Expanding this is real work with real calendar complexity (`session-to-now` and `same-day close` both need session-relative logic that does not exist). **It must exist before Stage 3 runs, and it deserves its own plan** — folding it in here would make Stage 2 unshippable. Tracked as open question **S2-B**.
- **The two-year fetch itself.** Stage 2 builds the *capability*; Stage 3 uses it.
- Any claim about whether sentiment predicts returns.

---

## Task 1 — Ship the scorer (LM + VADER)

**Why:** The blocker. Chosen by measurement, not reputation — see the bake-off. Takes blindness from 49%→2% (Massive) and 94%→27% (GDELT), resolution from 7→128 values, and directional bias from 14:1 to 4:1.

- [ ] Step 1: Embed the checked-in lexicons with `include_str!` — compile-time, not runtime file IO. A scorer that reads a file at runtime can silently change between the development year and the holdout year, which would invalidate the entire experiment.
- [ ] Step 2: Rewrite `sentiment.rs::score_text` as the combined scorer:

```rust
// LM tone, length-normalized so a 46-word description and an 11-word headline
// are comparable. sqrt (not len) because hit counts grow sub-linearly with text.
fn lm_tone(tokens: &[&str]) -> f64 {
    let p = tokens.iter().filter(|t| LM_POSITIVE.contains(*t)).count() as f64;
    let n = tokens.iter().filter(|t| LM_NEGATIVE.contains(*t)).count() as f64;
    if p + n == 0.0 { return 0.0; }
    (p - n) / (tokens.len() as f64).sqrt()
}

// VADER, with negation. "not strong" must not score positive — and headlines
// really do write that.
fn vader_compound(tokens: &[&str]) -> f64 { /* valence sum, negation flip, x/sqrt(x^2+15) */ }

pub fn score_text(text: &str) -> SentimentResult {
    let tokens = tokenize(text);
    let score = (0.5 * lm_tone(&tokens) + 0.5 * vader_compound(&tokens)).clamp(-1.0, 1.0);
    ...
}
```

- [ ] Step 3: `has_lexicon_hit` now means "contains any LM or VADER term". `lexicon_hit_rate` stays a first-class data-quality metric — it is what caught this in the first place.
- [ ] Step 4: **Bump `SENTIMENT_VERSION` to `stage2_lm_vader_v1`.** This is what keeps every existing `observation_set_id` honest: an observation set scored by a different scorer is a *different observation set*, and the manifest must say so.
- [ ] Step 5: Retune `VerdictThresholds::min_lexicon_hit_rate`. The old 0.20 was calibrated to a broken scorer and the old scorer cleared it *by 0.002*. With ~2% blindness on Massive the honest floor is much higher — propose **0.60**, and record that it is a *new* threshold, not a tightened one.

**Required tests:**
- `the_new_scorer_reads_the_headline_the_old_one_was_blind_to` — use a real headline from the sample that scored exactly 0.0 before.
- `negation_flips_polarity` — "not strong" must not score positive.
- `scores_stay_bounded_in_minus_one_to_one`
- `the_scorer_is_deterministic_across_runs`
- `lexicon_hit_rate_on_the_real_sample_exceeds_0_60` — the empirical claim, pinned. **If this fails, the scorer regressed.**
- `an_11_word_gdelt_headline_still_produces_a_nonzero_score` — the S2-A mitigation, pinned.

---

## Task 2 — Store vendor sentiment as a benchmark (never trade on it)

**Why:** Decision 21. Massive ships **per-ticker** sentiment with reasoning at 100% coverage, free. It is the only yardstick we will ever get for "is our local scorer any good" — and it is *not* a signal: it is categorical (3 values), so it cannot be quantiled at all (46% long / 54% short / **0% flat** — every neutral article shorted).

- [ ] Step 1: Parse `insights[]` from the Massive payload into `RawArticle.vendor_sentiment: Vec<VendorSentiment>` (`{ticker, label, reasoning}`). Per-ticker, not per-article — an article about "AMZN vs MSFT" genuinely has different sentiment for each.
- [ ] Step 2: Carry it onto `NormalizedArticle`. GDELT articles get an empty vec (they have none), and that absence must be *visible*, not silently zero.
- [ ] Step 3: Report `vendor_agreement` (Spearman ρ between local score and vendor label) as a **data-quality metric** in `summary.md`. A collapsing ρ is an early warning that the scorer has drifted from anything a reader would recognize.
- [ ] Step 4: **A compile-time-ish guarantee that it never reaches the strategy.** `backtest.rs` must not import it. Add a test asserting no `Trade` field derives from vendor sentiment.

**Required tests:**
- `vendor_sentiment_is_parsed_per_ticker_not_per_article`
- `a_gdelt_article_has_no_vendor_sentiment_and_says_so`
- `vendor_sentiment_never_reaches_the_backtest` — the guard. If someone wires it in, the whole research question silently changes from "does news sentiment predict returns" to "does Massive's black box predict returns".
- `vendor_agreement_on_the_real_sample_is_reported`

---

## Task 3 — Checkpoint: re-run the Stage 1 sample with the new scorer

**Why:** We have a frozen real sample and a known prior result (all 6 configurations → `revise`, every spread negative). Re-running it is the cheapest possible check that Tasks 1-2 did what the bake-off promised — *before* building anything on top.

- [ ] Step 1: `cargo run -- run --config configs/stage1_saved_sample.json`.
- [ ] Step 2: Record, in the findings doc: new `lexicon_hit_rate`, new degenerate count, new per-configuration verdicts, and `vendor_agreement`.
- [ ] Step 3: **Expect the verdicts to change. Do not expect them to improve.** A better scorer may well reveal *more clearly* that there is no signal in 5 days of data — that is a success, not a failure. **If the spreads suddenly turn positive, be suspicious, not pleased**: with 381 articles over one bull week, a positive result is far more likely to be noise or a bug than an edge.

---

## Task 4 — The 4 baselines (Decision 6's other half)

**Why:** The spec's failure gate is explicit: *"V1 should stop or revise when sentiment performs no better than shuffled or non-sentiment baselines."* We have **one** of four. Without the rest, `continue` means nothing — we cannot distinguish an edge from a coin that landed well.

- [ ] Step 1: Add `seed: u64` to `PipelineConfig`. The spec's Required Tests demand *"determinism for identical snapshot, configuration, and seed"* — a random baseline without a recorded seed is not reproducible, and an irreproducible baseline is not a baseline.
- [ ] Step 2: Implement the three missing baselines as alternative *signals* fed to the **same** backtest engine, so they are compared on identical trades, costs, and slot logic:

| baseline | signal |
|---|---|
| `always_flat` | never trade — the floor. If sentiment cannot beat *doing nothing after costs*, there is no edge. |
| `random` | seeded RNG in place of `mean_sentiment` |
| `prior_return_momentum` | `observation.prior_return` in place of `mean_sentiment` — the "you just rediscovered momentum" check |
| `shuffled_sentiment` | *(exists)* — real scores, permuted across observations |

- [ ] Step 3: `BacktestMetrics` gains a `strategy: String` field (`sentiment` \| `always_flat` \| …). Every configuration now emits **5 rows**, not 1.
- [ ] Step 4: **Wire the baselines into the verdict.** This is the point of building them:

```rust
// A positive spread that ANY non-sentiment baseline matches is not a finding.
if sentiment_net <= baselines.iter().map(|b| b.net).fold(f64::MIN, f64::max) {
    return decide("revise", "sentiment does not beat its best baseline: ...");
}
```

- [ ] Step 5: `summary.md` shows sentiment against each baseline, per configuration.

**Required tests:**
- `always_flat_takes_zero_trades_and_returns_zero`
- `the_random_baseline_is_reproducible_from_its_seed`
- `a_different_seed_gives_a_different_random_baseline` — otherwise the seed is decorative.
- `momentum_baseline_uses_prior_return_not_sentiment`
- `sentiment_that_merely_matches_its_best_baseline_yields_revise` — **the test that stops us fooling ourselves.**
- `all_five_strategies_are_backtested_on_identical_observations` — an unfair comparison is worse than none.

---

## Task 5 — The live-API source

**Why:** Stage 3 needs two years. The trait seam already exists (Decision 12), so **this touches no research code at all** — that was the entire point of building it.

- [ ] Step 1: Add `reqwest` (blocking). This is the first HTTP dependency in the binary, and it is deliberate: Stage 1 stayed offline *on purpose*, and that constraint has now served its purpose.
- [ ] Step 2: `src/source/api/{massive,gdelt,alpaca}.rs` implementing `NewsSource`/`PriceSource`. **Reuse the Stage 1 vendor parsers unchanged** — they are already proven against real payloads. The API source only *fetches*; parsing stays shared, so the saved-file and API paths cannot drift apart.
- [ ] Step 3: `SourceMode::Api`. Credentials from env, never from config, never logged.
- [ ] Step 4: **Pagination is mandatory, not optional.** Stage 1 learned this the hard way: ignoring Alpaca's `next_page_token` silently dropped 4 of 12 symbols, and *a missing symbol is indistinguishable from a symbol with no news*. Follow `next_url` (Massive) and `next_page_token` (Alpaca) to exhaustion, and **fail loudly** if a requested symbol returns nothing.

**Required tests:**
- `the_api_source_and_the_saved_file_source_produce_identical_normalized_rows` — feed both the same payload bytes. The spec requires fixture/file/API modes be indistinguishable downstream; this proves it.
- `pagination_is_followed_to_exhaustion`
- `a_requested_symbol_that_returns_no_bars_is_an_error_not_a_shrug`

---

## Task 6 — Rate limiting, retry, and resumable ingestion

**Why:** The spec: *"Make ingestion idempotent and resumable."* It is currently neither, and at Massive's ~5 req/min a two-year fetch is a **multi-hour** job. It will be interrupted — by a 429, a laptop lid, a flaky network. Restarting from zero each time is not a workflow, and re-fetching what we already have is both slow and rude.

- [ ] Step 1: A token-bucket limiter per vendor, rates in config (`massive: 5/min`, `alpaca: 200/min`, `gdelt: conservative`). Stage 1 hit 429s from **both** Massive *and* GDELT — GDELT rate-limits despite needing no key.
- [ ] Step 2: Exponential backoff on 429/5xx, bounded attempts, and **never** on 4xx-that-means-you (401/403 is a bad key: fail immediately and say so, rather than retrying five times into a wall).
- [ ] Step 3: **A raw-payload cache**, keyed by a hash of the request. On re-run, a cached request is not re-issued. This gives idempotence and resumability in one move, and it is also what makes the whole thing debuggable: the exact bytes the vendor sent are on disk.
- [ ] Step 4: `--resume` continues an interrupted ingest from the cache.

**Required tests:**
- `a_cached_request_is_not_reissued`
- `an_interrupted_ingest_resumes_without_refetching_what_it_already_has`
- `a_429_backs_off_and_retries`
- `a_401_fails_immediately_rather_than_retrying_into_a_wall`
- `two_identical_ingests_produce_the_same_dataset_id` — idempotence, mechanically.

---

## Task 7 — The spec's remaining Data Quality requirements

**Why:** Three requirements are still unimplemented, and all three only *matter* at Stage 3 scale — which is precisely why they must be built now, before we are staring at two years of partially-fetched data with no idea what is missing.

- [ ] Step 1: **An ingest report** — `runs/<run_id>/reports/ingest_log.csv`. The spec: *"Record rate-limit responses, retries, source outages, dropped rows, and coverage gaps."* Every request, its status, retries, and duration.
- [ ] Step 2: **Coverage gates.** The spec: *"Mark a run invalid when material coverage gaps violate its configured minimum."* Add `min_coverage` to config. A run below it is written with `valid: false` in its manifest and its summary **says so at the top** — because a report nobody flagged is a report somebody will believe.
- [ ] Step 3: **Keep failed runs.** The spec: *"Keep failed experiments and their diagnostics as first-class run artifacts."* An invalid run is not deleted; it is retained *with its diagnostics*. Failure is data.

**Required tests:**
- `a_rate_limited_request_appears_in_the_ingest_log`
- `a_run_below_min_coverage_is_marked_invalid`
- `an_invalid_run_still_writes_its_artifacts_and_says_why`

---

## Task 8 — Verify, demo, and record

- [ ] Step 1: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, all tests green.
- [ ] Step 2: The fixture path still produces its Stage 0 verdicts. **The `dataset_id` will move** (new scorer ⇒ new `SENTIMENT_VERSION` ⇒ new `observation_set_id`); the *verdicts* are what must be justified, and a changed verdict here needs an explanation, not a shrug.
- [ ] Step 3: A **live** narrow probe: one week, via the API source, resumed at least once mid-fetch to prove Task 6 works for real rather than only in tests.
- [ ] Step 4: Write `docs/superpowers/plans/2026-07-14-stage-2-findings.md`. Promote anything durable into design.md.

---

## Implementation Notes

- **The fixture and the saved sample are both regression suites now.** Stage 1's committed payloads mean every change can be checked against real data, offline, forever. Use them.
- **Expect the new scorer to change the verdicts, and do not expect it to improve them.** A better scorer may reveal more clearly that 5 days of data contains no signal. That is the honest outcome. **A sudden positive result should be treated as a bug until proven otherwise.**
- **Tasks 5-7 touch no research code.** If a diff in this stage modifies `analysis.rs`, `backtest.rs`, or `observations.rs` for a reason that is not the baselines (Task 4), something has gone wrong with the seam.

## Open Questions

| ID | Question | Blocks? | How to resolve |
|---|---|---|---|
| **S2-A** | Does GDELT's 27%-blind, 11-word text actually produce *usable* `broad_news` signal, or is it noise dressed as data? | Not Stage 2. The scorer mitigated this from 94%→27% blind. | Look at real `broad_news` results after Task 3. Options remain: GDELT's GKG tone field, a fuller-text mode, or accepting the weakness and recording it. |
| **S2-B** | **The First Experiment Matrix is far from complete.** We have 2 of ~3 news windows, **1 of 4** measurement horizons, **1 of 2** threshold pairs, **1 of 4** targets, and no 15-minute bars or doubled-cost stress case. | **Blocks Stage 3**, not Stage 2. | Needs its own plan. `session-to-now` and `same-day close` need session-relative logic that does not exist yet. Do not let Stage 3 start without it — running a two-year experiment on a quarter of the intended matrix would answer a question nobody asked. |
| Holdout | Is 1 year dev / 1 year holdout sufficient, and are the gates strong enough to act on? | Blocks Stage 3. | Still open since 2026-07-08. Task 4's baselines make the gates *materially* stronger — revisit once they exist. |
