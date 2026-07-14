use crate::config::Stage0Config;
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
        Commands::Run(args) => print_loaded_config(args.stage, "run"),
        Commands::Fixture(args) => print_loaded_config(args, "fixture"),
        Commands::BuildObservations(args) => print_loaded_config(args.stage, "build-observations"),
        Commands::Analyze(args) => print_loaded_config(args.stage, "analyze"),
        Commands::Backtest(args) => print_loaded_config(args.observation.stage, "backtest"),
    }
}

fn print_loaded_config(args: StageArgs, command_name: &str) -> Result<()> {
    let mut config = Stage0Config::load(&args.config)?;
    if let Some(output_root) = args.output_root {
        config.output_root = output_root.display().to_string();
    }
    println!(
        "command={command_name} run_id={} output_root={} dry_run={}",
        config.run_id, config.output_root, args.dry_run
    );
    Ok(())
}
