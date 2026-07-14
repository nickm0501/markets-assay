use crate::{config::Stage0Config, pipeline};
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "markets")]
#[command(about = "Correlation-first market sentiment research pipeline")]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Run(RunArgs),
    Fixture(StageArgs),
    BuildObservations(StageArgsWithDataset),
    Analyze(StageArgsWithObservationSet),
    Backtest(BacktestArgs),
}

#[derive(Debug, Parser)]
struct StageArgs {
    #[arg(long)]
    config: PathBuf,
    #[arg(long)]
    output_root: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

#[derive(Debug, Parser)]
struct StageArgsWithDataset {
    #[command(flatten)]
    stage: StageArgs,
    #[arg(long)]
    dataset_id: String,
}

// `run_id` here (and on `RunArgs` below) overrides `config.run_id` for this
// invocation only. It defaults to `config.run_id` when omitted. This exists
// because `run_id` identifies one analysis/backtest *configuration and
// result* (spec's Core Terminology; design.md's `configuration` term
// explicitly includes "one cost/threshold pair" within backtest) — without a
// way to name a rerun, every `backtest --cost-bps <n>` invocation would
// write to the same `runs/<run_id>/reports/*.csv` path and silently
// overwrite the previous cost's results, which defeats the Decision Demo's
// "compare both runs" step. See design.md Decision (2026-07-10, run_id
// overrides).
#[derive(Debug, Parser)]
struct StageArgsWithObservationSet {
    #[command(flatten)]
    stage: StageArgs,
    #[arg(long)]
    observation_set_id: String,
    #[arg(long)]
    run_id: Option<String>,
}

#[derive(Debug, Parser)]
struct RunArgs {
    #[command(flatten)]
    stage: StageArgs,
    #[arg(long)]
    run_id: Option<String>,
}

#[derive(Debug, Parser)]
struct BacktestArgs {
    #[command(flatten)]
    observation: StageArgsWithObservationSet,
    #[arg(long)]
    cost_bps: Option<f64>,
}

pub fn run_cli() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run(args) => {
            let config = Stage0Config::load(&args.stage.config)?;
            let run_id = args.run_id.clone().unwrap_or_else(|| config.run_id.clone());
            pipeline::run_all(&config, args.stage.output_root, args.stage.dry_run, &run_id)
        }
        Commands::Fixture(args) => run_loaded_config(args, pipeline::run_fixture),
        Commands::BuildObservations(args) => {
            let config = Stage0Config::load(&args.stage.config)?;
            pipeline::run_build_observations(
                &config,
                args.stage.output_root,
                args.stage.dry_run,
                &args.dataset_id,
            )
        }
        Commands::Analyze(args) => {
            let config = Stage0Config::load(&args.stage.config)?;
            let run_id = args.run_id.clone().unwrap_or_else(|| config.run_id.clone());
            pipeline::run_analyze(
                &config,
                args.stage.output_root,
                args.stage.dry_run,
                &args.observation_set_id,
                &run_id,
            )
        }
        Commands::Backtest(args) => {
            let config = Stage0Config::load(&args.observation.stage.config)?;
            let run_id = args
                .observation
                .run_id
                .clone()
                .unwrap_or_else(|| config.run_id.clone());
            pipeline::run_backtest_command(
                &config,
                args.observation.stage.output_root,
                args.observation.stage.dry_run,
                &args.observation.observation_set_id,
                args.cost_bps,
                &run_id,
            )
        }
    }
}

fn run_loaded_config(
    args: StageArgs,
    runner: fn(&Stage0Config, Option<PathBuf>, bool) -> Result<()>,
) -> Result<()> {
    let config = Stage0Config::load(&args.config)?;
    runner(&config, args.output_root, args.dry_run)
}
