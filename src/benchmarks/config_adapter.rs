use anyhow::Result;

use crate::benchmarks::config::{merge_benchmark_options, BenchmarkConfig, BenchmarkOptions};

/// Adapter to convert benchmark configuration to the format needed by the runner
pub struct ConfigAdapter;

impl ConfigAdapter {
    /// Get merged options for a benchmark
    pub fn get_merged_options(
        config: &BenchmarkConfig,
        benchmark_index: usize,
    ) -> Result<BenchmarkOptions> {
        let benchmark = &config.benchmarks[benchmark_index];
        merge_benchmark_options(&config.global.benchmark, &benchmark.benchmark)
    }
}
