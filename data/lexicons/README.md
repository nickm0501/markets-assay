# Sentiment Lexicons

Checked in, not downloaded at build time. The spec requires the sentiment scorer be
"deterministic, locally computed, versioned" — a lexicon fetched from the network at
build time is none of those things, and a two-year backtest whose scorer silently
changed underneath it is worthless.

Selected by measurement, not reputation: see
`docs/superpowers/design/correlation-first-pipeline/2026-07-14-sentiment-scorer-bakeoff.md`.

| file | source | contents |
|---|---|---|
| `loughran_mcdonald_positive.txt` | Loughran-McDonald Master Dictionary (1993-2021) | 354 words |
| `loughran_mcdonald_negative.txt` | same | 2,355 words |
| `vader_lexicon.txt` | VADER (cjhutto/vaderSentiment) | 7,506 entries, valence-scored |

**Why both.** They fail in different places, which is why the combination beat either
alone. Loughran-McDonald was built from 10-K filings and reads Massive's fuller
descriptions well, but is 66% blind on 11-word GDELT headlines. VADER was built for
short, punchy, emotive text — i.e. headlines — and is only 31% blind there. Together:
2% blind on Massive, 27% on GDELT.

Fetched 2026-07-14. Any change to these files MUST bump `SENTIMENT_VERSION`; that is
what keeps every previously built `observation_set_id` honest.
