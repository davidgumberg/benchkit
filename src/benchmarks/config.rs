use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::HashMap, path::PathBuf};

#[derive(Debug, Deserialize)]
pub struct GlobalConfig {
    pub hyperfine: Option<HashMap<String, Value>>,
    pub wrapper: Option<String>,
    pub source: PathBuf,
    pub branch: String,
    pub commits: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct SingleConfig {
    pub name: String,
    pub env: Option<HashMap<String, String>>,
    pub hyperfine: HashMap<String, Value>,
}

#[derive(Debug, Deserialize)]
pub struct BenchmarkConfig {
    pub global: GlobalConfig,
    pub benchmarks: Vec<SingleConfig>,
}

pub fn load_bench_config(bench_config_path: &PathBuf) -> Result<BenchmarkConfig> {
    if !bench_config_path.exists() {
        anyhow::bail!("Config file not found: {:?}", bench_config_path);
    }

    let config_dir = bench_config_path
        .parent()
        .context("Failed to get config directory")?;

    let contents = std::fs::read_to_string(bench_config_path)
        .with_context(|| format!("Failed to read config file: {:?}", bench_config_path))?;

    let mut config: BenchmarkConfig = serde_yaml::from_str(&contents)
        .with_context(|| format!("Failed to parse YAML from file: {:?}", bench_config_path))?;

    // Resolve relative paths to absolute paths
    if !config.global.source.is_absolute() {
        config.global.source = config_dir
            .join(&config.global.source)
            .canonicalize()
            .with_context(|| {
                format!("Failed to resolve source path: {:?}", config.global.source)
            })?;
    }
    println!("Using configuration\n{:?}", config);
    Ok(config)
}
