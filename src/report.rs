use crate::{analysis::AnalysisSummary, domain::run::BacktestMetrics};
use anyhow::Result;
use plotters::prelude::*;
use std::{fs, path::Path};

/// One row per (news_window, measurement_horizon, source_set) configuration.
/// Never collapse this into a single blended verdict (design.md Decision 1) —
/// configurations are meant to be compared, not merged.
pub fn write_summary(
    path: &Path,
    dataset_id: &str,
    observation_set_id: &str,
    analyses: &[AnalysisSummary],
    metrics: &[BacktestMetrics],
) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let continue_count = analyses
        .iter()
        .filter(|analysis| analysis.recommendation == "continue")
        .count();
    let quarantine_rate = analyses
        .first()
        .map(|analysis| analysis.quarantine_rate)
        .unwrap_or(0.0);
    let lexicon_hit_rate = analyses
        .first()
        .map(|analysis| analysis.lexicon_hit_rate)
        .unwrap_or(0.0);
    let degenerate_count = analyses
        .iter()
        .filter(|analysis| analysis.degenerate)
        .count();
    let vendor_agreement = analyses
        .first()
        .map(|analysis| analysis.vendor_agreement)
        .unwrap_or(0.0);

    let mut text = format!(
        "# Research Summary\n\n\
         dataset_id: {dataset_id}\n\n\
         observation_set_id: {observation_set_id}\n\n\
         configurations: {}\n\n\
         continue: {continue_count}\n\n\
         ## Data Quality\n\n\
         Data-quality gates are evaluated BEFORE signal gates: a spread computed over\n\
         unusable data is not a weak result, it is not a result at all.\n\n\
         | metric | value | meaning |\n\
         |---|---|---|\n\
         | quarantine_rate | {quarantine_rate:.4} | share of articles with broken/missing timestamps. Drives `stop`. Scope exclusions (out-of-window, wrong symbol, duplicate) are NOT counted here. |\n\
         | lexicon_hit_rate | {lexicon_hit_rate:.4} | share of articles the sentiment scorer could actually read. A low value means we are not measuring sentiment, we are measuring silence. |\n\
         | vendor_agreement | {vendor_agreement:.4} | Spearman rho between our local scorer and the VENDOR's own read. A BENCHMARK, never traded on. A collapsing value means the scorer has drifted from anything a reader would recognise. |\n\
         | degenerate_configurations | {degenerate_count} | configurations whose sentiment scores were too tied to separate a top from a bottom. These take ZERO trades rather than an all-long book. |\n\n\
         See `reports/set_aside.csv` for every article that did not become an observation,\n\
         and `reports/timestamp_audit.csv` for published_at vs available_at per article.\n\n\
         ## Per-Configuration Verdicts\n\n\
         Each row is one (news_window, measurement_horizon, source_set) configuration.\n\
         Configurations are never blended into a single verdict (design.md Decision 1).\n\
         Long and short sides are reported separately as well as combined (spec Backtest Rules).\n\n\
         | news_window_minutes | measurement_horizon_minutes | source_set | observations | recommendation | reason | sentiment_net | best_baseline | best_baseline_net | degenerate | articles_per_signal | observed_top_minus_bottom | shuffled_top_minus_bottom | pearson | trades | net_return_sum | win_rate | long_trades | long_net_return_sum | long_win_rate | short_trades | short_net_return_sum | short_win_rate |\n\
         |---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|\n",
        analyses.len(),
    );
    for analysis in analyses {
        let matching_metrics = metrics.iter().find(|metric| {
            metric.news_window_minutes == analysis.news_window_minutes
                && metric.measurement_horizon_minutes == analysis.measurement_horizon_minutes
                && metric.source_set == analysis.source_set
        });
        let (
            trade_count,
            net_return_sum,
            win_rate,
            long_count,
            long_net_return_sum,
            long_win_rate,
            short_count,
            short_net_return_sum,
            short_win_rate,
        ) = matching_metrics
            .map(|metric| {
                (
                    metric.trade_count,
                    metric.net_return_sum,
                    metric.win_rate,
                    metric.long_count,
                    metric.long_net_return_sum,
                    metric.long_win_rate,
                    metric.short_count,
                    metric.short_net_return_sum,
                    metric.short_win_rate,
                )
            })
            .unwrap_or((0, 0.0, 0.0, 0, 0.0, 0.0, 0, 0.0, 0.0));
        text.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {:.6} | {} | {:.6} | {} | {:.2} | {:.6} | {:.6} | {:.4} | {} | {:.6} | {:.2} | {} | {:.6} | {:.2} | {} | {:.6} | {:.2} |\n",
            analysis.news_window_minutes,
            analysis.measurement_horizon_minutes,
            analysis.source_set,
            analysis.observation_count,
            analysis.recommendation,
            analysis.reason,
            analysis.sentiment_net_return,
            analysis.best_baseline_name,
            analysis.best_baseline_net_return,
            analysis.degenerate,
            analysis.articles_per_signal,
            analysis.observed_top_minus_bottom,
            analysis.shuffled_top_minus_bottom,
            analysis.pearson_correlation,
            trade_count,
            net_return_sum,
            win_rate,
            long_count,
            long_net_return_sum,
            long_win_rate,
            short_count,
            short_net_return_sum,
            short_win_rate,
        ));
    }
    fs::write(path, text)?;
    Ok(())
}

pub fn write_bucket_chart(path: &Path, rows: &[(String, f64)]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let root = SVGBackend::new(path, (800, 420)).into_drawing_area();
    root.fill(&WHITE)?;
    let y_min = rows.iter().map(|(_, v)| *v).fold(0.0_f64, f64::min) - 0.005;
    let y_max = rows.iter().map(|(_, v)| *v).fold(0.0_f64, f64::max) + 0.005;
    let mut chart = ChartBuilder::on(&root)
        .caption("Bucket Returns", ("sans-serif", 24))
        .margin(20)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(0..rows.len(), y_min..y_max)?;
    chart.configure_mesh().draw()?;
    chart.draw_series(rows.iter().enumerate().map(|(idx, (_, value))| {
        Rectangle::new([(idx, 0.0), (idx + 1, *value)], BLUE.mix(0.65).filled())
    }))?;
    root.present()?;
    Ok(())
}

pub fn write_equity_curve(path: &Path, equity: &[f64]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let root = SVGBackend::new(path, (800, 420)).into_drawing_area();
    root.fill(&WHITE)?;
    let y_min = equity.iter().copied().fold(0.0_f64, f64::min) - 0.005;
    let y_max = equity.iter().copied().fold(0.0_f64, f64::max) + 0.005;
    let mut chart = ChartBuilder::on(&root)
        .caption("Equity Curve", ("sans-serif", 24))
        .margin(20)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(0..equity.len(), y_min..y_max)?;
    chart.configure_mesh().draw()?;
    chart.draw_series(LineSeries::new(
        equity.iter().enumerate().map(|(idx, value)| (idx, *value)),
        &GREEN,
    ))?;
    root.present()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{analysis::AnalysisSummary, domain::run::BacktestMetrics};
    use tempfile::TempDir;

    #[test]
    fn summary_markdown_lists_one_row_per_configuration() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("summary.md");
        let analyses = vec![
            AnalysisSummary {
                news_window_minutes: 60,
                measurement_horizon_minutes: 60,
                source_set: "finance_only".into(),
                observation_count: 4,
                observed_top_minus_bottom: 0.01,
                shuffled_top_minus_bottom: 0.0,
                shuffled_p95: 0.0,
                pearson_correlation: 0.4,
                quarantine_rate: 0.0,
                articles_per_signal: 2.0,
                source_set_coverage: 1.0,
                lexicon_hit_rate: 0.9,
                degenerate: false,
                vendor_agreement: 0.5,
                sentiment_net_return: 0.03,
                best_baseline_net_return: 0.01,
                best_baseline_name: "prior_return_momentum".into(),
                recommendation: "continue".into(),
                reason: "observed spread beats the shuffled baseline".into(),
            },
            AnalysisSummary {
                news_window_minutes: 240,
                measurement_horizon_minutes: 60,
                source_set: "broad_news".into(),
                observation_count: 3,
                observed_top_minus_bottom: -0.002,
                shuffled_top_minus_bottom: 0.001,
                shuffled_p95: 0.002,
                pearson_correlation: -0.1,
                quarantine_rate: 0.0,
                articles_per_signal: 2.0,
                source_set_coverage: 1.0,
                lexicon_hit_rate: 0.9,
                degenerate: false,
                vendor_agreement: 0.5,
                sentiment_net_return: 0.005,
                best_baseline_net_return: 0.02,
                best_baseline_name: "prior_return_momentum".into(),
                recommendation: "revise".into(),
                reason: "observed spread does not beat the shuffled baseline".into(),
            },
        ];
        let metrics = vec![
            BacktestMetrics {
                run_id: "stage0_fixture".into(),
                news_window_minutes: 60,
                measurement_horizon_minutes: 60,
                source_set: "finance_only".into(),
                strategy: "sentiment".into(),
                split: "holdout".into(),
                cost_bps: 5.0,
                trade_count: 4,
                long_count: 2,
                short_count: 2,
                gross_return_sum: 0.03,
                net_return_sum: 0.028,
                average_net_return: 0.007,
                win_rate: 0.75,
                profit_factor: 3.0,
                max_drawdown: -0.002,
                long_gross_return_sum: 0.02,
                long_net_return_sum: 0.019,
                long_win_rate: 1.0,
                long_profit_factor: 19.0,
                short_gross_return_sum: 0.01,
                short_net_return_sum: 0.009,
                short_win_rate: 0.5,
                short_profit_factor: 1.5,
                degenerate: false,
            },
            BacktestMetrics {
                run_id: "stage0_fixture".into(),
                news_window_minutes: 240,
                measurement_horizon_minutes: 60,
                source_set: "broad_news".into(),
                strategy: "sentiment".into(),
                split: "holdout".into(),
                cost_bps: 5.0,
                trade_count: 3,
                long_count: 1,
                short_count: 1,
                gross_return_sum: -0.001,
                net_return_sum: -0.002,
                average_net_return: -0.0007,
                win_rate: 0.33,
                profit_factor: 0.5,
                max_drawdown: -0.004,
                long_gross_return_sum: 0.0005,
                long_net_return_sum: 0.0,
                long_win_rate: 0.0,
                long_profit_factor: 0.0,
                short_gross_return_sum: -0.0015,
                short_net_return_sum: -0.002,
                short_win_rate: 0.0,
                short_profit_factor: 0.0,
                degenerate: false,
            },
        ];

        write_summary(&path, "ds_test", "obs_test", &analyses, &metrics).unwrap();
        let text = std::fs::read_to_string(path).unwrap();

        assert!(text.contains("dataset_id: ds_test"));
        assert!(text.contains("observation_set_id: obs_test"));
        assert!(text.contains("configurations: 2"));
        assert!(text.contains("| 60 | 60 | finance_only |"));
        assert!(text.contains("continue"));
        assert!(text.contains("revise"));
        assert!(text.contains("long_net_return_sum"));
        assert!(text.contains("short_net_return_sum"));
    }

    #[test]
    fn svg_chart_files_are_written() {
        let temp = TempDir::new().unwrap();
        write_bucket_chart(
            &temp.path().join("bucket_returns.svg"),
            &[("low".into(), -0.01), ("high".into(), 0.02)],
        )
        .unwrap();
        write_equity_curve(&temp.path().join("equity_curve.svg"), &[0.0, 0.01, 0.015]).unwrap();

        assert!(
            std::fs::read_to_string(temp.path().join("bucket_returns.svg"))
                .unwrap()
                .contains("<svg")
        );
        assert!(
            std::fs::read_to_string(temp.path().join("equity_curve.svg"))
                .unwrap()
                .contains("<svg")
        );
    }
}
