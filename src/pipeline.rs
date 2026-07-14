use crate::{config::Stage0Config, fixture::generate_fixture};
use anyhow::Result;
use std::path::PathBuf;

pub fn run_fixture(
    config: &Stage0Config,
    output_root: Option<PathBuf>,
    dry_run: bool,
) -> Result<()> {
    let fixture = generate_fixture(config)?;
    let root = output_root.unwrap_or_else(|| PathBuf::from(&config.output_root));
    println!(
        "fixture run_id={} output_root={} dry_run={} articles={} price_bars={}",
        config.run_id,
        root.display(),
        dry_run,
        fixture.raw_articles.len(),
        fixture.price_bars.len()
    );
    Ok(())
}
