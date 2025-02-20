use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use shellexpand;
use std::{collections::HashMap, path::PathBuf};

#[derive(Debug, Deserialize, Clone)]
pub struct GlobalConfig {
    pub hyperfine: Option<HashMap<String, Value>>,
    pub wrapper: Option<String>,
    pub source: PathBuf,
    pub branch: String,
    pub commits: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SingleConfig {
    pub name: String,
    pub env: Option<HashMap<String, String>>,
    pub hyperfine: HashMap<String, Value>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BenchmarkConfig {
    pub global: GlobalConfig,
    pub benchmarks: Vec<SingleConfig>,
}

fn expand_path(path: &str) -> String {
    shellexpand::full(path)
        .unwrap_or_else(|_| path.into())
        .into_owned()
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

    // First expand any environment variables in the source path
    let expanded_source = expand_path(config.global.source.to_str().unwrap_or(""));
    config.global.source = PathBuf::from(expanded_source);

    // Then resolve relative paths to absolute paths
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
