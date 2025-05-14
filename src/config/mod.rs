/// Application configuration
pub mod app;
pub use app::{load_app_config, AppConfig};

/// Benchmark configuration
pub mod benchmark;
pub use benchmark::{
    load_bench_config, merge_benchmark_options, BenchmarkConfig, BenchmarkGlobalConfig,
    BenchmarkOptions, SingleConfig,
};

/// Configuration adapter for merging options
pub mod adapter;
pub use adapter::ConfigAdapter;

/// Benchmark options extensions for merging
mod benchmark_ext;

/// Merging traits and utilities
pub mod merge;

/// Configuration traits
pub mod traits;
pub use traits::{Configuration, MergeableConfiguration, PathConfiguration};

/// Tests for configuration
#[cfg(test)]
mod tests;

/// Global configuration containing both app and benchmark configurations
#[derive(Debug, Clone)]
pub struct GlobalConfig {
    /// Application configuration
    pub app: AppConfig,
    /// Benchmark configuration
    pub bench: BenchmarkConfig,
}

impl Configuration for GlobalConfig {
    fn config_path(&self) -> &std::path::PathBuf {
        // The GlobalConfig doesn't have its own path, so we return the benchmark config path
        self.bench.config_path()
    }

    fn config_type(&self) -> &str {
        "global"
    }

    fn validate(&self) -> anyhow::Result<()> {
        // Validate both app and benchmark configurations
        self.app.validate()?;
        self.bench.validate()?;

        Ok(())
    }
}
