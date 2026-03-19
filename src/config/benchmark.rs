use anyhow::{Context, Result};
use log::debug;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, path::PathBuf};

use crate::path_utils::process_path;

/// Configuration for benchmark runs
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BenchmarkOptions {
    #[serde(default = "default_warmup")]
    pub warmup: usize,
    #[serde(default = "default_runs")]
    pub runs: usize,
    #[serde(default)]
    pub capture_output: bool,
    pub command: Option<String>,
    pub parameter_lists: Option<Vec<Value>>,
    pub profile: Option<bool>,
    pub profile_interval: Option<u64>,
    pub stop_on_log_pattern: Option<String>,
    pub perf_instrumentation: Option<bool>,
    pub flamegraph: Option<bool>,
}

const fn default_warmup() -> usize {
    0
}

const fn default_runs() -> usize {
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
            perf_instrumentation: None,
            flamegraph: None,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if let (Some(true), Some(interval)) = (self.profile, self.profile_interval) {
            if interval == 0 {
                anyhow::bail!("Profile interval cannot be zero");
            }
        }

        if let Some(pattern) = &self.stop_on_log_pattern {
            if pattern.is_empty() {
                anyhow::bail!("stop_on_log_pattern cannot be empty");
            }
            match Regex::new(pattern) {
                Ok(_) => {}
                Err(e) => anyhow::bail!("Invalid regex pattern in stop_on_log_pattern: {}", e),
            }
        }

        // Validate perf instrumentation is only enabled on Linux
        if let Some(true) = self.perf_instrumentation {
            #[cfg(not(target_os = "linux"))]
            {
                anyhow::bail!("perf_instrumentation is only supported on Linux");
            }
        }

        // Validate mutual exclusion of instrumentation modes
        let modes_enabled = [
            self.profile.unwrap_or(false),
            self.perf_instrumentation.unwrap_or(false),
            self.flamegraph.unwrap_or(false),
        ];
        let count = modes_enabled.iter().filter(|&&v| v).count();
        if count > 1 {
            anyhow::bail!(
                "Only one instrumentation mode can be enabled at a time. \
                 Found {} of: profile, perf_instrumentation, flamegraph",
                count
            );
        }


        Ok(())
    }

    pub fn validate_for_execution(&self) -> Result<()> {
        self.validate()?;
        if self.command.is_none() {
            anyhow::bail!("Benchmark is missing a command template");
        }
        Ok(())
    }

    fn merge_from_map(&self, map: &HashMap<String, Value>) -> Result<Self> {
        let mut result = self.clone();

        if let Some(warmup) = map.get("warmup").and_then(|v| v.as_u64()) {
            result.warmup = warmup as usize;
        }

        if let Some(runs) = map.get("runs").and_then(|v| v.as_u64()) {
            result.runs = runs as usize;
        }

        if let Some(capture_output) = map.get("capture_output").and_then(|v| v.as_bool()) {
            result.capture_output = capture_output;
        }

        if let Some(command) = map.get("command").and_then(|v| v.as_str()) {
            result.command = Some(command.to_string());
        }

        if let Some(parameter_lists) = map.get("parameter_lists").and_then(|v| v.as_array()) {
            result.parameter_lists = Some(parameter_lists.clone());
        }

        if let Some(profile) = map.get("profile").and_then(|v| v.as_bool()) {
            result.profile = Some(profile);
        }

        if let Some(profile_interval) = map.get("profile_interval").and_then(|v| v.as_u64()) {
            result.profile_interval = Some(profile_interval);
        }

        if let Some(stop_on_log_pattern) = map.get("stop_on_log_pattern").and_then(|v| v.as_str()) {
            result.stop_on_log_pattern = Some(stop_on_log_pattern.to_string());
        }

        if let Some(perf_instrumentation) =
            map.get("perf_instrumentation").and_then(|v| v.as_bool())
        {
            result.perf_instrumentation = Some(perf_instrumentation);
        }

        if let Some(flamegraph) = map.get("flamegraph").and_then(|v| v.as_bool()) {
            result.flamegraph = Some(flamegraph);
        }

        Ok(result)
    }
}

/// Global configuration for all benchmarks
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BenchmarkGlobalConfig {
    pub benchmark: Option<BenchmarkOptions>,
    pub benchmark_cores: Option<String>,
    pub runner_cores: Option<String>,
    pub cmake_build_args: Option<Vec<String>>,
    pub source: PathBuf,
    pub commits: Vec<String>,
}

/// Configuration for a single benchmark
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SingleConfig {
    pub name: String,
    pub env: Option<HashMap<String, String>>,
    pub network: String,
    pub connect: Option<String>,
    pub benchmark: HashMap<String, Value>,
}

/// Complete benchmark configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BenchmarkConfig {
    pub global: BenchmarkGlobalConfig,
    pub benchmarks: Vec<SingleConfig>,
    #[serde(default)]
    pub path: PathBuf,
}

// Deserialize and validate benchmark config from a YAML string.
pub fn parse_bench_config(s: &str) -> Result<BenchmarkConfig> {
    let config: BenchmarkConfig = serde_yaml::from_str(s)
        .with_context(|| "Failed to parse YAML.")?;
    validate_config(&config)?;

    Ok(config)
}

/// Load benchmark configuration from a YAML file
pub fn load_bench_config(bench_config_path: &PathBuf) -> Result<BenchmarkConfig> {
    if !bench_config_path.exists() {
        anyhow::bail!("Benchmark config file not found: {:?}", bench_config_path);
    }

    let config_dir = bench_config_path
        .parent()
        .context("Failed to get benchmark config directory")?;

    let contents = std::fs::read_to_string(bench_config_path)
        .with_context(|| format!("Failed to read benchmark config file: {bench_config_path:?}"))?;

    let mut config =  parse_bench_config(&contents)
        .with_context(|| format!("Failed to parse YAML from file: {bench_config_path:?}"))?;

    config.path = bench_config_path.to_path_buf();

    // Expand paths in global config
    let source_str = config.global.source.to_string_lossy().to_string();
    let is_url = source_str.starts_with("http:")
        || source_str.starts_with("https:")
        || source_str.starts_with("git:")
        || source_str.starts_with("git@");


    if !is_url {
        // Only expand non-URL paths
        process_path(&mut config.global.source, config_dir, true)?;
    }


    debug!(
        "Loaded benchmark configuration from {:?}",
        bench_config_path
    );
    Ok(config)
}

fn validate_config(config: &BenchmarkConfig) -> Result<()> {
    // Validate global options
    if let Some(opts) = &config.global.benchmark {
        opts.validate()?;
    }

    if config.global.commits.is_empty() {
        anyhow::bail!("No commits specified for benchmarking");
    }

    // Validate CPU core specifications
    if let Some(cores) = &config.global.benchmark_cores {
        if !is_valid_cpu_cores(cores) {
            anyhow::bail!("Invalid benchmark_cores format: {}", cores);
        }
    }

    if let Some(cores) = &config.global.runner_cores {
        if !is_valid_cpu_cores(cores) {
            anyhow::bail!("Invalid runner_cores format: {}", cores);
        }
    }

    // Validate benchmarks
    if config.benchmarks.is_empty() {
        anyhow::bail!("No benchmarks configured");
    }

    for benchmark in &config.benchmarks {
        if benchmark.name.is_empty() {
            anyhow::bail!("Benchmark name cannot be empty");
        }

        match benchmark.network.as_str() {
            "main" | "test" | "signet" | "regtest" => {}
            _ => anyhow::bail!("Invalid network type: {}", benchmark.network),
        }
    }

    Ok(())
}

/// Merge global and benchmark-specific options
pub fn merge_benchmark_options(
    global_opts: &Option<BenchmarkOptions>,
    benchmark_opts: &HashMap<String, Value>,
) -> Result<BenchmarkOptions> {
    let base_opts = global_opts.clone().unwrap_or_default();
    base_opts.merge_from_map(benchmark_opts)
}

/// Get merged options for a benchmark
pub fn get_merged_options(
    config: &BenchmarkConfig,
    benchmark_index: usize,
) -> Result<BenchmarkOptions> {
    let benchmark = &config.benchmarks[benchmark_index];
    let options = merge_benchmark_options(&config.global.benchmark, &benchmark.benchmark)?;
    options.validate_for_execution()?;
    Ok(options)
}

fn is_valid_cpu_cores(cores: &str) -> bool {
    for part in cores.split(',') {
        if part.contains('-') {
            let range: Vec<&str> = part.split('-').collect();
            if range.len() != 2
                || range[0].parse::<usize>().is_err()
                || range[1].parse::<usize>().is_err()
            {
                return false;
            }
        } else if part.parse::<usize>().is_err() {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_benchmark_options_merge() {
        let base_opts = BenchmarkOptions {
            warmup: 1,
            runs: 2,
            capture_output: false,
            command: Some("base command".to_string()),
            parameter_lists: None,
            profile: Some(false),
            profile_interval: Some(5),
            stop_on_log_pattern: None,
            perf_instrumentation: None,
            flamegraph: None,
        };

        let mut override_map = HashMap::new();
        override_map.insert("warmup".to_string(), Value::from(3));
        override_map.insert("runs".to_string(), Value::from(4));
        override_map.insert("capture_output".to_string(), Value::from(true));
        override_map.insert("command".to_string(), Value::from("override command"));
        override_map.insert("profile".to_string(), Value::from(true));

        let merged = base_opts.merge_from_map(&override_map).unwrap();

        assert_eq!(merged.warmup, 3);
        assert_eq!(merged.runs, 4);
        assert!(merged.capture_output);
        assert_eq!(merged.command, Some("override command".to_string()));
        assert_eq!(merged.profile, Some(true));
        assert_eq!(merged.profile_interval, Some(5)); // Unchanged
    }

    #[test]
    fn test_is_valid_cpu_cores() {
        assert!(is_valid_cpu_cores("0"));
        assert!(is_valid_cpu_cores("0,1,2"));
        assert!(is_valid_cpu_cores("0-3"));
        assert!(is_valid_cpu_cores("0-3,5,7-9"));

        assert!(!is_valid_cpu_cores(""));
        assert!(!is_valid_cpu_cores("a"));
        assert!(!is_valid_cpu_cores("0-"));
        assert!(!is_valid_cpu_cores("-3"));
    }
}
