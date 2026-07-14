pub mod analysis;
pub mod audit;
pub mod backtest;
pub mod calendar;
pub mod cli;
pub mod config;
pub mod domain;
pub mod ids;
pub mod normalize;
pub mod observations;
pub mod pipeline;
pub mod report;
pub mod sentiment;
pub mod source;
pub mod storage;
pub mod verdict;

pub use cli::run_cli;
