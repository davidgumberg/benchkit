use anyhow::{Context, Result};
use log::debug;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, path::PathBuf};

use crate::config::merge::MergeFromMap;
use crate::config::traits::{Configuration, MergeableConfiguration, PathConfiguration};
use crate::path_utils;

/// Configuration for benchmark runs
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BenchmarkOptions {
    /// Number of warmup runs to perform (discarded from results)
    #[serde(default = "default_warmup")]
    pub warmup: usize,
    /// Number of measured runs to perform
    #[serde(default = "default_runs")]
    pub runs: usize,
    /// Whether to capture and store command output
    #[serde(default)]
    pub capture_output: bool,
    /// The command template to execute
    pub command: Option<String>,
    /// Lists of parameters to substitute in the command
    pub parameter_lists: Option<Vec<Value>>,
    /// Whether to enable profiling for this benchmark
    pub profile: Option<bool>,
    /// Sampling interval for profiling in seconds
    pub profile_interval: Option<u64>,
    /// Optional regex pattern to stop the benchmark when matched in log output
    pub stop_on_log_pattern: Option<String>,
}

fn default_warmup() -> usize {
    0
}

fn default_runs() -> usize {
    1
}

impl Default for BenchmarkOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl BenchmarkOptions {
    pub fn new() -> Self {
        Self {
            warmup: default_warmup(),
            runs: default_runs(),
            capture_output: false,
            command: None,
            parameter_lists: None,
            profile: None,
            profile_interval: None,
            stop_on_log_pattern: None,
        }
    }

    /// Validate the benchmark options
    pub fn validate(&self) -> anyhow::Result<()> {
        // Ensure the profile interval is reasonable if profiling is enabled
        if let (Some(true), Some(interval)) = (self.profile, self.profile_interval) {
            if interval == 0 {
                anyhow::bail!("Profile interval cannot be zero");
            }
        }

        // Validate stop_on_log_pattern if present
        if let Some(pattern) = &self.stop_on_log_pattern {
            if pattern.is_empty() {
                anyhow::bail!("stop_on_log_pattern cannot be empty");
            }
            // Validate that it's a valid regex pattern
            use regex::Regex;
            match Regex::new(pattern) {
                Ok(_) => {}
                Err(e) => anyhow::bail!("Invalid regex pattern in stop_on_log_pattern: {}", e),
            }
        }

        Ok(())
    }

    /// Validate for execution - command must be present
    pub fn validate_for_execution(&self) -> anyhow::Result<()> {
        self.validate()?;
        if self.command.is_none() {
            anyhow::bail!("Benchmark is missing a command template");
        }
        Ok(())
    }
}

impl MergeableConfiguration<HashMap<String, Value>> for BenchmarkOptions {
    /// Merges benchmark options with a HashMap of options
    fn merge_with(&self, other: &HashMap<String, Value>) -> anyhow::Result<Self> {
        self.merge_from_map(other)
    }
}

/// Global configuration for all benchmarks
#[derive(Debug, Deserialize, Clone)]
pub struct BenchmarkGlobalConfig {
    /// Default benchmark options
    pub benchmark: Option<BenchmarkOptions>,
    /// CPU cores to run the benchmark on
    pub benchmark_cores: Option<String>,
    /// CPU cores to run the main program on
    pub runner_cores: Option<String>,
    /// Custom CMake build arguments
    pub cmake_build_args: Option<Vec<String>>,
    /// Path to source code repository
    pub source: PathBuf,
    /// Path to scratch directory for building
    pub scratch: PathBuf,
    /// Git commits to benchmark
    pub commits: Vec<String>,
    /// Temporary data directory for benchmark runs
    pub tmp_data_dir: PathBuf,
}

// Thread-local to store an empty path for BenchmarkGlobalConfig
thread_local! {
    static EMPTY_PATH: PathBuf = PathBuf::new();
}

impl Configuration for BenchmarkGlobalConfig {
    fn config_path(&self) -> &PathBuf {
        // BenchmarkGlobalConfig doesn't have its own path,
        // but this method is required to satisfy the trait.
        // In practice, it's used through BenchmarkConfig which has a path.
        // This is a bit of a hack but works for our limited use case
        Box::leak(Box::new(EMPTY_PATH.with(|p| p.clone())))
    }

    fn config_type(&self) -> &str {
        "benchmark_global"
    }

    fn validate(&self) -> anyhow::Result<()> {
        if let Some(opts) = &self.benchmark {
            opts.validate()?;
        }
        if self.commits.is_empty() {
            anyhow::bail!("No commits specified for benchmarking");
        }
        if let Some(cores) = &self.benchmark_cores {
            if !is_valid_cpu_cores(cores) {
                anyhow::bail!("Invalid benchmark_cores format: {}", cores);
            }
        }
        if let Some(cores) = &self.runner_cores {
            if !is_valid_cpu_cores(cores) {
                anyhow::bail!("Invalid runner_cores format: {}", cores);
            }
        }

        Ok(())
    }
}

impl PathConfiguration for BenchmarkGlobalConfig {
    fn paths_for_expansion(&self) -> Vec<&PathBuf> {
        vec![&self.source, &self.scratch, &self.tmp_data_dir]
    }

    fn with_expanded_paths(&self, config_dir: &std::path::Path) -> anyhow::Result<Self> {
        let mut config = self.clone();

        // Check if source is a URL before processing as a path
        let source_str = config.source.to_string_lossy().to_string();
        let is_url = source_str.starts_with("http:")
            || source_str.starts_with("https:")
            || source_str.starts_with("git:")
            || source_str.starts_with("git@");

        if is_url {
            // Only process scratch and tmp_data_dir if source is a URL
            path_utils::process_paths(
                &mut [&mut config.scratch, &mut config.tmp_data_dir],
                config_dir,
                true,
            )?;
        } else {
            // Process all paths including source for local paths
            path_utils::process_paths(
                &mut [
                    &mut config.source,
                    &mut config.scratch,
                    &mut config.tmp_data_dir,
                ],
                config_dir,
                true,
            )?;
        }

        Ok(config)
    }
}

/// Configuration for a single benchmark
#[derive(Debug, Deserialize, Clone)]
pub struct SingleConfig {
    /// Benchmark name
    pub name: String,
    /// Environment variables to set
    pub env: Option<HashMap<String, String>>,
    /// Network to use (main, test, signet, etc.)
    pub network: String,
    /// Address to connect to
    pub connect: Option<String>,
    /// Benchmark-specific options (overrides global options)
    pub benchmark: HashMap<String, Value>,
}

impl SingleConfig {
    /// Validate single benchmark configuration
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.name.is_empty() {
            anyhow::bail!("Benchmark name cannot be empty");
        }

        // Validate network value
        match self.network.as_str() {
            "main" | "test" | "signet" | "regtest" => {}
            _ => anyhow::bail!("Invalid network type: {}", self.network),
        }

        Ok(())
    }
}

/// Complete benchmark configuration
#[derive(Debug, Deserialize, Clone)]
pub struct BenchmarkConfig {
    /// Global configuration options
    pub global: BenchmarkGlobalConfig,
    /// List of benchmarks to run
    pub benchmarks: Vec<SingleConfig>,
    /// Unique run ID
    #[serde(default)]
    pub run_id: i64,
    #[serde(default)]
    pub path: PathBuf,
}

impl Configuration for BenchmarkConfig {
    fn config_path(&self) -> &PathBuf {
        &self.path
    }

    fn config_type(&self) -> &str {
        "benchmark"
    }

    fn validate(&self) -> anyhow::Result<()> {
        // Validate global configuration
        self.global.validate()?;
        for benchmark in &self.benchmarks {
            benchmark.validate()?;
        }
        if self.benchmarks.is_empty() {
            anyhow::bail!("No benchmarks configured");
        }
        Ok(())
    }
}

// Helper function to validate CPU core specifications
pub fn is_valid_cpu_cores(cores: &str) -> bool {
    // Accepts formats like "0", "0-3", "0,1,2", "0-2,4,6-8"
    let mut valid = true;
    for part in cores.split(',') {
        if part.contains('-') {
            let range: Vec<&str> = part.split('-').collect();
            if range.len() != 2
                || range[0].parse::<usize>().is_err()
                || range[1].parse::<usize>().is_err()
            {
                valid = false;
                break;
            }
        } else if part.parse::<usize>().is_err() {
            valid = false;
            break;
        }
    }
    valid
}

/// Load benchmark configuration from a YAML file
pub fn load_bench_config(bench_config_path: &PathBuf, run_id: i64) -> Result<BenchmarkConfig> {
    if !bench_config_path.exists() {
        anyhow::bail!("Benchmark config file not found: {:?}", bench_config_path);
    }
    let config_dir = bench_config_path
        .parent()
        .context("Failed to get benchmark config directory")?;
    let contents = std::fs::read_to_string(bench_config_path).with_context(|| {
        format!(
            "Failed to read benchmark config file: {:?}",
            bench_config_path
        )
    })?;
    let mut config: BenchmarkConfig = serde_yaml::from_str(&contents)
        .with_context(|| format!("Failed to parse YAML from file: {:?}", bench_config_path))?;
    config.run_id = run_id;
    config.path = bench_config_path.to_path_buf();
    let global_config = config.global.with_expanded_paths(config_dir)?;
    config.global = global_config;
    config.validate()?;
    debug!("Using {} configuration\n{:?}", config.config_type(), config);
    Ok(config)
}

/// Merge global and benchmark-specific options
pub fn merge_benchmark_options(
    global_opts: &Option<BenchmarkOptions>,
    benchmark_opts: &HashMap<String, Value>,
) -> Result<BenchmarkOptions> {
    let base_opts = global_opts.clone().unwrap_or_default();
    // Merge specific options into global options
    base_opts.merge_with(benchmark_opts)
}
