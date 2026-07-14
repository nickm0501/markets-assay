# Stage 1 Saved Sample — Provenance and Findings

Real vendor payloads, retained **unchanged**. Fetched 2026-07-14 by
`scripts/fetch_sample.sh`. Hand-editing performed: **none**.

## Window

- **Symbols:** SPY, QQQ, DIA, IWM (ETFs) + AAPL, AMZN, AMD, AVGO, GOOGL, MSFT, NVDA, TSLA
- **Dates:** 2025-07-01 → 2025-07-07
- **Why this week:** it contains a weekend, a real NYSE **early close** (Thu 2025-07-03,
  13:00 ET) and a real **holiday** (Fri 2025-07-04). Confirmed in the data: bars exist for
  07-01, 07-02, 07-03, 07-07 only, and 07-03's last bar is 17:00Z = 13:00 ET. The
  calendar logic (design.md Decision 2) met real dates and got them right.

## Files

| File | Vendor | Content |
|---|---|---|
| `massive_*.json` (12) | Massive | `/v2/reference/news`, 217 finance articles |
| `gdelt_macro.json` | GDELT | DOC 2.0 ArtList, 250 broad/macro articles |
| `alpaca_bars.json` | Alpaca | `/v2/stocks/bars` 1Hour, 345 bars, 12 symbols, IEX feed |

**Alpaca's free tier is the IEX feed**, not consolidated SIP — roughly 2-3% of US volume.
Bars are built from a slice of the tape. Fine for timestamp inspection; **not** fine to
trust as prices by Stage 3.

---

## The answers

### S1-A — Bar alignment: **CLOCK-ALIGNED (`:00`), and they include pre/post-market.** ✅ fixed

Every bar sits on the hour. The NYSE opens at **09:30**, so the bars do **not** align to
the session:

```
12:00Z = 08:00 ET   PRE-MARKET
13:00Z = 09:00 ET   STRADDLES THE OPEN — half pre-market, half regular
14:00Z = 10:00 ET   regular
...
20:00Z = 16:00 ET   the close / after-hours
```

The feared failure — a silent mass-drop from grid misalignment — **does not happen**:
`observations.rs` derives `signal_time` from the bar itself, so the signal grid *is* the
bar grid.

But two real problems did surface, and both are fixed:

1. **Non-regular bars could open trades.** The `12:00Z` (pre-market) and `20:00Z` (close)
   bars, and especially the `13:00Z` bar whose `open` is a **09:00 pre-market price that
   never traded in the regular session**, were all valid entry bars. That silently
   violates the spec's *"defer after-hours signals to the next regular-session tradable
   bar."* Non-regular bars are now kept as context but **cannot be entry bars**.

2. **After-hours bars would have silently reversed Decision 3.** Decision 3 says a horizon
   the session close prevents from covering must be *dropped*. That worked on the fixture
   only because bars ran out at the close. With real after-hours bars available, the
   contiguity check would have happily **bridged the close** using them. Measurement bars
   are now required to be regular-session too.

### S1-B — Syndication: **NONE in this sample.** ✅ no work needed

Of **132 distinct titles**, **0** appear under more than one URL. The URL+title dedupe key
is sufficient here. **Do not build near-duplicate machinery** — the data does not have the
problem. (Revisit at Stage 2/3, where a broader source mix may introduce it.)

Dedupe still did real work: **85 exact duplicates** were excluded. Those are a *fetch*
artifact — an article tagged `[AAPL, MSFT]` is returned by both `ticker=AAPL` and
`ticker=MSFT`, so it lands in two payload files. Correctly collapsed, and correctly
counted as `excluded / duplicate` rather than vanishing.

### S1-C — The sentiment lexicon: **reads 1 in 5 real headlines. It is not fit for purpose.** ⚠️ Stage 2 blocker

`lexicon_hit_rate = 0.2021`.

The 14-word lexicon understands **20%** of real headlines. The other 80% score exactly
`0.0` — indistinguishable from genuinely neutral news. We are not measuring sentiment; we
are largely measuring silence.

It cleared the `min_lexicon_hit_rate = 0.20` gate **by 0.002**. That is luck, not health.
The threshold should probably be raised, and **the scorer must be replaced before Stage 2**
draws any conclusion from a real dataset.

The degeneracy guard (design.md Decision 17) did *not* fire — averaging many articles per
signal produced enough spread to keep the quantiles apart. So the pipeline did not emit an
all-long book. But it came closer than anyone should be comfortable with.

---

## What the run actually said

**All 6 configurations → `revise`.** Every observed top-minus-bottom spread was *negative*,
and none beat its shuffled baseline. On this sample, **sentiment carries no signal.**

That is the correct and expected outcome. A 5-day, 12-symbol sample cannot establish
signal, and a 14-word lexicon reading 20% of headlines could not find it if it were there.
**These numbers are mechanically valid and statistically meaningless. Do not quote them.**

| | |
|---|---|
| raw articles | 467 |
| normalized | 381 |
| **quarantined** (broken timestamps) | **0** |
| excluded (scope) | 86 — 85 duplicate, 1 outside window |
| price bars | 345 across 12 symbols |
| trades | 442 (194 long / 248 short) |
| no-lookahead check | **PASSED** on real data |

### The timestamp finding nobody predicted

**301 of 381 articles (79%) are published outside the regular session** and had to be
deferred to the next session open. Median deferral: **823 minutes (~14 hours)**. Max: 5,550
minutes (a Thursday-evening article waiting through the holiday *and* the weekend).

Financial news is overwhelmingly written overnight and pre-market. The after-hours deferral
rule is not an edge case for this data — **it is the common path**, and it governs the
majority of every signal. Decision 4's `available_at` machinery is load-bearing in a way the
fixture (1 deferred article of 5) badly under-represented.

**Zero quarantined articles.** Every Massive and GDELT timestamp parsed cleanly. The
quarantine path is correct but was not exercised by this sample.

---

## STAGE 2 RESULT: sentiment loses to momentum

With the new scorer (LM+VADER, `lexicon_hit_rate` 0.20 → **0.82**) and all four
baselines in place, the real sample says:

| configuration | sentiment | always_flat | random | **momentum** | shuffled | winner |
|---|---|---|---|---|---|---|
| broad_news w=240 | -0.041 | -0.000 | -0.022 | **+0.023** | +0.005 | momentum |
| finance_only w=240 | +0.023 | -0.000 | +0.044 | **+0.070** | -0.018 | momentum |
| finance_plus_broad w=240 | +0.034 | -0.000 | -0.020 | +0.066 | **+0.113** | shuffled |
| broad_news w=60 | -0.018 | -0.000 | +0.004 | **+0.016** | -0.007 | momentum |
| finance_only w=60 | +0.032 | -0.000 | +0.005 | +0.079 | **+0.098** | shuffled |
| finance_plus_broad w=60 | +0.024 | -0.000 | -0.086 | **+0.098** | -0.009 | momentum |

**Sentiment wins 0 of 6.** `prior_return_momentum` beats it in **4 of 6**;
sentiment's own scores, *randomly shuffled*, beat it in the other 2.

**All 6 configurations: `revise`.**

Before the baselines existed, this sample reported a **`continue`** — a +0.045%
spread that beat only the shuffled baseline. It was a false positive, and the
baselines caught it. That is precisely what Decision 6's gate was for, and why
`continue` meant nothing until it existed.

Two caveats, in both directions:

- **This does not prove sentiment is worthless.** Five days, one bull week, 381
  articles. It proves nothing either way — that is what Stage 3's two years are
  for.
- **It does prove the gates work.** A pipeline that reports `continue` on noise
  is worse than useless, because someone will believe it. This one now doesn't.

## The hand-trace (spec: Required Tests)

One real article, followed from the vendor's raw JSON to the trade it caused.
Pinned as `tests/stage1_real_sample.rs`, so it cannot silently rot.

**The Motley Fool — "Which AI Stocks May Soar After Reaching Record Highs?"**, tagged
`[AVGO]`, published `2025-07-01T08:10:00Z`.

| step | value |
|---|---|
| **published_at** | `08:10Z` — 04:10 ET, **overnight, market shut** |
| **available_at** | `13:30Z` — deferred 320 min to the session open |
| **sentiment** | **+0.5** — the lexicon found exactly two words it knows, `growth` and `strong`, in the description (`+2/4`) |
| **signal_time** | `14:00Z` (10:00 ET, a regular-session bar). The 60-min window is `(13:00Z, 14:00Z]`, and `13:30Z` falls inside it |
| **entry bar** | AVGO `14:00Z`: open **269.69** → close **263.95** |
| **future_return** | `(263.95 − 269.69) / 269.69` = **−0.02128** |
| **trade** | **LONG** (sentiment cleared the top quantile). Gross **−2.13%**, net **−2.23%** after 10bps |

Two things this makes concrete that no aggregate could:

1. **The article reached the signal *because* it was deferred, not despite it.** Published
   overnight, it could not inform anything until the market opened. `available_at` is what
   put it in the 14:00Z window. Decision 4's machinery is not a corner case here — it is
   the mechanism by which most of this dataset works at all.

2. **The scorer read the headline "Which AI Stocks May Soar After Reaching Record Highs?"
   and scored it strongly positive on the strength of two tokens.** It has no idea what the
   article says. It went long. AVGO fell 2.1% in the hour. This single row *is* the S1-C
   argument.

## The Decision Demo (spec §Decision Demo)

Verified on the real sample: ingest once → backtest at 0bps → change **one** cost
assumption → rerun at 10bps **without re-ingesting** (same `observation_set_id`) → compare
both runs side by side. `--run-id` keeps the two runs' reports separate, so they can
actually be compared rather than one overwriting the other.

Costs behave exactly as arithmetic demands — e.g. `finance_only w=240` loses `0.104` more
at 10bps across 104 trades, which is `104 × 0.001`.

Every configuration was already losing money before costs. Costs make it worse. Verdict
across the board: **`revise`**.

## Three bugs real data found that the plan did not anticipate

1. **Massive tags news to companies, not ETFs.** Only **4 of 677** articles in the week
   mentioned SPY/QQQ/DIA/IWM; AMZN alone had 57. The original ETF-only universe (Decision
   13) would have produced almost no observations. Fixed by adding 8 large-caps and a
   `constituent_etf_map` so an AMZN story propagates to QQQ/SPY — which is the spec's own
   relevance design, not a workaround.

2. **Alpaca paginates, and we ignored the cursor.** The first fetch silently returned 8 of
   12 symbols — SPY, QQQ, NVDA and TSLA simply vanished. A missing symbol is
   indistinguishable from a symbol with no news. The script now follows `next_page_token`
   and **refuses to write a partial sample**.

3. **The Parquet schema depended on which enum variants the data happened to contain.**
   Real data produced no `NewsScope::SectorTheme` article at all (Massive articles always
   carry tickers; GDELT never does), leaving a hole in the middle of the variant indices.
   serde_arrow emitted a `Null`-typed placeholder column and refused to write the snapshot.
   The fixture survived only by luck — its unobserved variant happened to be the *last* one.
   The domain enums now serialize as plain strings, so the schema depends on the type, not
   on the data.
