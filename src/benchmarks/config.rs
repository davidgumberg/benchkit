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
    pub bin_dir: PathBuf,
}

#[derive(Debug, Deserialize)]
pub struct BenchmarkConfig {
    pub name: String,
    pub env: Option<HashMap<String, String>>,
    pub hyperfine: HashMap<String, Value>,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub global: GlobalConfig,
    pub benchmarks: Vec<BenchmarkConfig>,
}

pub fn load_config(config_path: &PathBuf) -> Result<Config> {
    if !config_path.exists() {
        anyhow::bail!("Config file not found: {:?}", config_path);
    }

    let config_dir = config_path
        .parent()
        .context("Failed to get config directory")?;

    let contents = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read config file: {:?}", config_path))?;

    let mut config: Config = serde_yaml::from_str(&contents)
        .with_context(|| format!("Failed to parse YAML from file: {:?}", config_path))?;

    // Resolve relative paths to absolute paths
    if !config.global.source.is_absolute() {
        config.global.source = config_dir
            .join(&config.global.source)
            .canonicalize()
            .with_context(|| {
                format!("Failed to resolve source path: {:?}", config.global.source)
            })?;
    }

    if !config.global.bin_dir.is_absolute() {
        config.global.bin_dir = config_dir
            .join(&config.global.bin_dir)
            .canonicalize()
            .with_context(|| {
                format!(
                    "Failed to resolve out_dir path: {:?}",
                    config.global.bin_dir
                )
            })?;
    }
    println!("Using configuration\n{:?}", config);
    Ok(config)
}
