# Markets

Local research pipeline for testing whether news sentiment has predictive value for equity and ETF trades.

## Stage 0 Fixture Demo

Run the deterministic synthetic pipeline:

```bash
cargo run -- run --config configs/stage0_fixture.json
```

Generated outputs:

- `artifacts/data/datasets/<dataset_id>/`
- `artifacts/data/observation_sets/<observation_set_id>/`
- `artifacts/runs/stage0_fixture/reports/summary.md`
- `artifacts/runs/stage0_fixture/charts/`

Rerun a changed cost assumption without rebuilding the dataset, comparing both runs by giving the rerun its own `--run-id` (otherwise it overwrites `runs/stage0_fixture/`):

```bash
cargo run -- backtest \
  --config configs/stage0_fixture.json \
  --observation-set-id <observation_set_id> \
  --cost-bps 10 \
  --run-id stage0_fixture_cost10

diff artifacts/runs/stage0_fixture/reports/backtest_metrics.csv \
     artifacts/runs/stage0_fixture_cost10/reports/backtest_metrics.csv
```
