# Experiment: Sentiment Scorer Bake-Off

Status: **Planned — awaiting permission to run**
Date: 2026-07-14
Blocks: Stage 2, and therefore Stage 3's go/no-go.

## Question

**Which deterministic, locally computed sentiment scorer can actually rank real
financial headlines well enough for a quantile-based long/short rule to work?**

Not "which scorer is most accurate." The strategy never uses the absolute score —
it takes the top and bottom quantiles of the score *distribution*. So the only
property that matters is **ordering power**: can the scorer separate a top
quintile from a bottom quintile at all, without a pile of ties in the middle?

## Conjecture

A finance-specific lexicon (Loughran-McDonald), scored **continuously** rather
than in fixed steps, will fix three of the four measured failures — the
directional bias, the blindness, and the lack of resolution — and will agree with
the vendor's per-ticker labels substantially better than the current 14-word
lexicon.

It will **not** fix GDELT (see Limitations). If that conjecture is wrong — if a
finance lexicon built from 10-K filings does not transfer to 11-to-46-word news
text — we need to know that *before* building a two-year pipeline on it.

## Why logic and existing evidence are not enough

We have the diagnosis but not the answer. What we know from Stage 1's real sample:

| failure | measured |
|---|---|
| directional bias | 101 positive vs **7** negative on Massive (14:1) |
| blindness (scores exactly 0.0) | **49%** of Massive articles, **94%** of GDELT |
| resolution | **7 discrete values**, 0.25 apart |
| overall | `lexicon_hit_rate = 0.2021` — reads 1 headline in 5 |

What we cannot know by reasoning:

- Loughran-McDonald was built from **10-K filings** — long, formal, legalistic
  prose. Our text is headlines plus a 1-2 sentence description. **Whether that
  vocabulary transfers is an empirical question, not a deducible one.**
- VADER was built for **social media** — it handles negation ("not strong"),
  intensifiers, and punctuation, which headlines do use. But its vocabulary is
  general, not financial: it does not know that "beat", "miss", "guidance" or
  "downgrade" carry financial polarity.
- Adopting either without measuring would repeat **exactly the mistake that let
  the 14-word lexicon survive all the way to Stage 1**: a scorer that was never
  evaluated against real text.

## Method

1. Implement each candidate behind the existing scorer seam (`sentiment.rs`
   already has one entry point, `score_text`).
2. Score all **381 normalized articles** from the committed Stage 1 sample.
3. Evaluate each candidate on the four criteria below.
4. Record results in this file. Choose on evidence, then hand off to a plan.

### Candidates

| id | scorer |
|---|---|
| **A** | Current 14-word lexicon (the control — we must show improvement *against* something) |
| **B** | Loughran-McDonald, continuous tone |
| **C** | VADER, continuous compound score |
| **D** | LM + VADER combined |
| **E** | **Vendor label + local tone** — the hybrid |

### Why vendor sentiment cannot be the signal on its own (measured, 2026-07-14)

Worth stating plainly, because "just use the vendor's sentiment, it's free" is the
obvious move and it **fails for a concrete mechanical reason**, not a dogmatic one.

Massive's sentiment is **free** (132/132 articles on the free tier) and genuinely
good — an LLM read the article and left its reasoning. But it is **categorical**:
`positive` / `neutral` / `negative`. Three values. Feed that to the quantile rule:

```
short_threshold = quantile(0.20) =  0   (neutral)
long_threshold  = quantile(0.80) = +1   (positive)

  LONG   189 (46%)
  SHORT  219 (54%)   <-- every NEUTRAL article is shorted
  FLAT     0 ( 0%)
```

The spec's rule is *"long the upper quantile, short the lower quantile, **remain
flat in the middle**."* **Vendor sentiment has no middle.** With three values the
thresholds land on category boundaries, the flat zone vanishes, and every neutral
article gets shorted. This is the *same* degeneracy failure as the 14-word lexicon
(`is_degenerate`), reached from the opposite direction: not too few words, but too
few *values*.

Its other two gaps:

- **Zero GDELT coverage** — 65% of the corpus has no vendor label at all.
- **Time-varying model.** It is LLM-generated. Over a two-year Stage 3 snapshot,
  Massive may have scored 2024 articles with one model and 2025 with another. A
  scorer that changes over time manufactures spurious signal across the
  development/holdout boundary — which is exactly the boundary the entire
  experiment rests on.

**Candidate E takes the good part anyway.** The vendor label supplies *semantic
judgment*; a local continuous score supplies the *resolution* it lacks:

```
score = vendor_label (+1 / 0 / -1)  +  w * local_tone   # w < 1, so the
                                                        # label dominates and
                                                        # tone breaks ties
```

This ranks *within* each vendor category, restoring the flat zone. It still cannot
score GDELT — so if E wins, S2-A (GDELT text starvation) becomes urgent rather than
merely open.

### The yardstick — and why we suddenly have one

Massive ships **per-ticker sentiment with reasoning at 100% coverage** of its
articles: **410 labels** across 132 articles. The spec explicitly permits this
("vendor sentiment may be stored as a benchmark but is not ground truth"), and it
is the closest thing to ground truth we will ever get for free.

**We are not trading on it.** We are using it to answer a question we currently
have no way to answer at all: *is our local scorer any good?*

### Result structure

For each candidate, on the real corpus:

| metric | why it matters | today (control) |
|---|---|---|
| **ranking power** — Spearman ρ vs vendor labels, and whether a top vs bottom quintile can be formed at all | This is the ONLY property the strategy uses. A scorer that cannot rank is useless no matter how "accurate". | cannot rank (7 buckets, 54% ties) |
| **blind-rate** — share scoring exactly 0.0 | A blind article is indistinguishable from a neutral one. | 49% Massive / 94% GDELT |
| **directional balance** — positive:negative ratio | 14:1 means the long book fills on vocabulary, not news. | 101:7 |
| **agreement** — accuracy / F1 vs the 410 vendor labels | Sanity: does it agree with a human-ish reading? | unmeasured |
| **degenerate-rate** — how often `is_degenerate` would fire | Directly predicts whether Stage 3 can trade at all. | ~fires on thin slices |

**A candidate wins only if it can rank.** Everything else is secondary.

## Scope

- Uses the **already-committed** Stage 1 sample. No new fetching, no API keys.
- Lexicon data files (LM, VADER) are downloaded once and **checked in**, exactly
  like the vendor payloads — so the scorer stays deterministic, versioned, and
  offline-reproducible, per the spec.

## Affected files

**Disposable** (deleted after the experiment):
- a scratch bake-off harness

**Retained if a candidate wins**:
- `data/lexicons/*` — the winning lexicon, checked in and versioned
- `src/sentiment.rs` — the new scorer, behind the existing seam
- `SENTIMENT_VERSION` — bumped (this is what makes an observation set honest)
- This file — with the observations filled in

**Not touched**: the research loop. `analysis.rs`, `backtest.rs`, `observations.rs`
never learn which scorer produced a score.

## Expected effort

Half a day. The corpus, the seam, and the yardstick all already exist.

## Limitations — what this experiment CANNOT settle

**GDELT is a text problem, not a scorer problem, and no candidate will fix it.**

GDELT gives us **title only, averaging 11 words**, and that is **65% of the
corpus** (249 of 381 articles). At 94% blind today, even a perfect scorer has
almost nothing to read. Swapping the lexicon will move that number a little; it
cannot solve it.

That is a **separate decision** and it needs its own design pass:

- drop GDELT, and with it the `broad_news` and `finance_plus_broad` source sets;
- find a GDELT mode that returns more text or its own tone score;
- accept title-only scoring for macro news and record the weakness honestly;
- replace the broad-news source entirely.

This experiment does not touch that question. It must not be read as having
answered it.

**Also out of scope:** whether sentiment predicts returns at all. This measures
whether we can *read* the news, not whether reading it is worth anything.

## Observations

Run 2026-07-14 against all 381 committed Stage 1 articles.
Lexicons: Loughran-McDonald (354 positive, 2,355 negative), VADER (7,506 entries).

| candidate | ρ vs vendor | Massive blind | **GDELT blind** | distinct values | pos:neg |
|---|---|---|---|---|---|
| **A** 14-word *(control)* | 0.339 | 54% | **94%** | 7 | 57:4 *(14:1)* |
| **B** Loughran-McDonald | 0.433 | 25% | 66% | 74 | 67:32 |
| **C** VADER | 0.500 | 3% | 31% | 85 | 109:19 |
| **D** **LM + VADER** | **0.519** | **2%** | **27%** | **128** | 105:25 *(4:1)* |
| ~~E~~ vendor + local tone | ~~0.974~~ | 1% | **n/a** | 131 | 107:24 |

### Candidate E's score is CIRCULAR and was discarded

E's ρ = 0.974 is **not evidence**. E *contains* the vendor label, and ρ measures
agreement *with the vendor label*. It was always going to score ~1.0. Reporting it
as a win would have been self-deception, and it is recorded here only so nobody
re-derives it and believes it.

E cannot be evaluated against the vendor benchmark **in principle**, because it is
not independent of it. Judging it would need a different ground truth — and the
only other one available is *"does it predict returns"*, which we **must not use**
(see below).

### The conjecture was WRONG about GDELT

The plan asserted GDELT was "a text problem, not a scorer problem" and that "no
candidate will fix it." **That was too pessimistic, and the measurement caught it.**

D takes GDELT from **94% blind → 27% blind**, and from **3 distinct values → 101**.

The reason, in hindsight: **VADER is built for short, punchy, emotive text** — which
is exactly what a headline *is*. LM, built from 10-K filings, does noticeably worse
on titles (66% blind) and better on Massive's fuller descriptions. They fail in
different places, which is precisely why combining them beats either alone.

GDELT is *mitigated*, not solved. 27% blind on 11-word titles is still thin, and
S2-A stays open — but it is no longer catastrophic, and it no longer threatens two
thirds of the experiment matrix.

### Every diagnosed defect is addressed

| defect | before | after (D) |
|---|---|---|
| directional bias | 101:7 *(14:1)* | *(4:1)* — **less skewed than the vendor benchmark itself (6.5:1)** |
| blindness (Massive) | 49% | **2%** |
| blindness (GDELT) | 94% | **27%** |
| resolution | 7 values | **128 values** |
| ranking power | cannot rank | **ranks** |

## Conclusion

**Adopt candidate D: Loughran-McDonald + VADER, continuous, equally weighted.**

It wins on every criterion that was set *before* the data was seen, and it beats
the hybrid (E) on grounds that have nothing to do with the score:

- **Coverage.** D reads 100% of the corpus. E reads 35% — Massive only, no GDELT.
- **Reproducibility.** D is a checked-in word list: deterministic, versioned,
  frozen. E depends on Massive's LLM, which may have scored 2024 and 2025 articles
  with *different models* — a time-varying scorer that manufactures spurious signal
  across the very development/holdout boundary the experiment rests on.
- **Independence.** E cannot be evaluated against our only benchmark without
  circularity.

### The methodological line, stated once so it is never crossed

**The scorer must NEVER be selected by which one predicts returns best.**

That would fit the scorer to the outcome and invalidate the entire experiment — it
is the same error as tuning on the holdout, wearing a different hat. Every criterion
in this bake-off (agreement with an independent benchmark, blindness, bias,
resolution) is deliberately **independent of returns**. Whether sentiment predicts
anything is the question we are trying to *answer*, not a knob we get to turn.

### What this does NOT prove

- Nothing about whether sentiment predicts returns. This measured whether we can
  *read* the news, not whether reading it is worth anything.
- The vendor benchmark is itself LLM-generated, not human ground truth. ρ = 0.519
  means "agrees moderately with an LLM's reading", which is a sanity check, not a
  certificate.
- One week, 381 articles, a bull week (the vendor's own labels skew 6.5:1 positive).
  Coverage and resolution should generalize; the *bias* figures may not.

## Artifact updates

- design.md: new decision recording the adoption of D, and S2-A downgraded from
  "no scorer fixes this" to "mitigated 94% → 27%, still open".
- `data/lexicons/` — LM and VADER word lists, checked in and versioned.
- `SENTIMENT_VERSION` must be bumped when D ships. That is what keeps every
  existing `observation_set_id` honest.
