use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;

use crate::database::DatabaseConfig;

#[derive(Debug, Deserialize)]
pub struct AppConfig {
    pub home_dir: PathBuf,
    pub bin_dir: PathBuf,
    pub database: DatabaseConfig,
}

pub fn load_app_config(app_config_path: &PathBuf) -> Result<AppConfig> {
    if !app_config_path.exists() {
        anyhow::bail!("App config file not found: {:?}", app_config_path);
    }

    let config_dir = app_config_path
        .parent()
        .context("Failed to get app config directory")?;

    let contents = std::fs::read_to_string(app_config_path)
        .with_context(|| format!("Failed to read app config file: {:?}", app_config_path))?;

    let mut config: AppConfig = serde_yaml::from_str(&contents)
        .with_context(|| format!("Failed to parse YAML from file: {:?}", app_config_path))?;

    // Resolve relative paths to absolute paths
    for path in [&mut config.home_dir, &mut config.bin_dir].iter_mut() {
        if !path.is_absolute() {
            **path = config_dir
                .join(&path)
                .canonicalize()
                .with_context(|| format!("Failed to resolve path: {:?}", path))?;
        }
    }
    println!("Using app configuration\n{:?}", config);
    Ok(config)
}
