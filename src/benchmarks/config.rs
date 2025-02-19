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

    let contents = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read config file: {:?}", config_path))?;
    serde_yaml::from_str(&contents)
        .with_context(|| format!("Failed to parse YAML from file: {:?}", config_path))
}
