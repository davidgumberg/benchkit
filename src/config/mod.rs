use anyhow::{Context, Result};
use log::debug;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tempfile::TempDir;

use crate::path_utils;

/// Enum to hold user-selected tmpdir vs our own.
#[derive(Debug, Clone)]
pub enum TmpDataDir {
    User(PathBuf),
    Temporary(Arc<TempDir>),
}

impl TmpDataDir {
    pub fn path(&self) -> &std::path::Path {
        match self {
            Self::User(p) => p,
            Self::Temporary(t) => t.path(),
        }
    }
}

/// Just for making the compiler happy.
impl Default for TmpDataDir {
    fn default() -> Self {
        Self::User(PathBuf::new())
    }
}

impl Serialize for TmpDataDir {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.path().to_string_lossy())
    }
}

/// For deserializing Application configuration loaded from config.yml
/// and finalizing into an AppConfig.
#[derive(Debug, Deserialize)]
pub struct RawAppConfig {
    /// The only mandatory field in AppConfig serialization,
    /// specifies the parent directory for binaries and build
    /// artifacts.
    pub scratch_dir: PathBuf,
    /// Optional in serialization, under scratch_dir by default.
    pub bin_dir: Option<PathBuf>,
    /// Optional in serialization, a tmpdir by default.
    pub tmp_datadir: Option<PathBuf>,
}

impl RawAppConfig {
    /// Convert a serialization-only `RawAppConfig` into a real one, avoid
    /// business logic here, just path expansion / parsing stuff, everything
    /// else goes in AppConfig::new()
    pub fn finalize(self, path: &PathBuf) -> Result<AppConfig> {
        let config_dir = path
            .parent()
            .context("Failed to get app config directory")?;

        let scratch_dir = expand_path(&self.scratch_dir, config_dir)?;

        // Expand the paths (only if they were passed by the user), otherwise
        let bin_dir = self.bin_dir
            .map(|p| expand_path(&p, config_dir))
            .transpose()?;

        let tmp_datadir = self.tmp_datadir
            .map(|p| expand_path(&p, config_dir))
            .transpose()?;

        AppConfig::new(
            path.to_path_buf(),
            scratch_dir,
            bin_dir,
            tmp_datadir
        )
    }
}

#[derive(Debug, Default, Serialize, Clone)]
pub struct AppConfig {
    #[serde(default)]
    pub path: PathBuf,
    pub scratch_dir: PathBuf,
    pub bin_dir: PathBuf,
    pub tmp_datadir: TmpDataDir,
}

impl AppConfig {
    pub fn new(path: PathBuf, scratch_dir: PathBuf, bin_dir: Option<PathBuf>, tmp_datadir: Option<PathBuf>) -> Result<Self> {
        let bin_dir = bin_dir.unwrap_or_else(|| scratch_dir.join("binaries"));
        let tmp_datadir = match tmp_datadir {
            Some(p) => TmpDataDir::User(p),
            None => {
                let t = tempfile::Builder::new()
                    .prefix("benchkit-")
                    .tempdir()
                    .context("Failed to create RAII temporary data directory")?;
                TmpDataDir::Temporary(Arc::new(t))
            }
        };
        Ok(Self {
            path,
            scratch_dir,
            bin_dir,
            tmp_datadir,
        })
    }
}

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
            perf_instrumentation: None,
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

/// Global configuration containing both app and benchmark configurations
#[derive(Debug, Clone)]
pub struct GlobalConfig {
    pub app: AppConfig,
    pub bench: BenchmarkConfig,
}

/// Load application configuration from a YAML file
pub fn load_app_config(app_config_path: &PathBuf) -> Result<AppConfig> {
    if !app_config_path.exists() {
        anyhow::bail!("App config file not found: {:?}", app_config_path);
    }


    let contents = std::fs::read_to_string(app_config_path)
        .with_context(|| format!("Failed to read app config file: {app_config_path:?}"))?;

    let raw_config: RawAppConfig = serde_yaml::from_str(&contents)
        .with_context(|| format!("Failed to parse YAML from file: {app_config_path:?}"))?;

    let config = raw_config.finalize(app_config_path)?;

    for dir in [&config.bin_dir] {
        if !dir.exists() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("Failed to create directory: {}", dir.display()))?;
        }
    }

    debug!(
        "Loaded application configuration from {:?}",
        app_config_path
    );
    Ok(config)
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
        expand_path(&mut config.global.source, config_dir)?;
    }


    debug!(
        "Loaded benchmark configuration from {:?}",
        bench_config_path
    );
    Ok(config)
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

fn expand_path(path: &PathBuf, config_dir: &std::path::Path) -> Result<PathBuf> {
    path_utils::process_path(path, config_dir, true)
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
    use std::fs;
    use std::io::Write;
    use tempfile::tempdir;

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
    fn test_load_app_config() {
        let tempdir = tempdir().unwrap();
        let config_path = tempdir.path().join("config.yml");

        let config_content = r#"
        scratch_dir: ./scratch
        bin_dir: ./bin
        "#;

        let mut file = fs::File::create(&config_path).unwrap();
        file.write_all(config_content.as_bytes()).unwrap();

        let config = load_app_config(&config_path).unwrap();

        assert!(config.bin_dir.is_absolute());
        assert_eq!(config.path, config_path);
    }
}
