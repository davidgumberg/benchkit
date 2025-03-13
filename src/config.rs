use anyhow::{Context, Result};
use log::debug;
use serde::Deserialize;
use std::path::PathBuf;

use shellexpand;

use crate::{benchmarks::BenchmarkConfig, database::DatabaseConfig};

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub bin_dir: PathBuf,
    pub database: Option<DatabaseConfig>,
    pub home_dir: PathBuf,
    pub patch_dir: PathBuf,
    pub snapshot_dir: PathBuf,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GlobalConfig {
    pub app: AppConfig,
    pub bench: BenchmarkConfig,
}

fn expand_path(path: &str) -> String {
    shellexpand::full(path)
        .unwrap_or_else(|_| path.into())
        .into_owned()
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

    // First expand any environment variables in the source path
    let expanded_home = expand_path(config.home_dir.to_str().unwrap_or(""));
    config.home_dir = PathBuf::from(expanded_home);
    let expanded_bin_dir = expand_path(config.bin_dir.to_str().unwrap_or(""));
    config.bin_dir = PathBuf::from(expanded_bin_dir);
    let expanded_snapshot_dir = expand_path(config.snapshot_dir.to_str().unwrap_or(""));
    config.snapshot_dir = PathBuf::from(expanded_snapshot_dir);
    let expanded_patch_dir = expand_path(config.patch_dir.to_str().unwrap_or(""));
    config.patch_dir = PathBuf::from(expanded_patch_dir);

    // Resolve relative paths to absolute paths and create directories
    for path in [
        &mut config.bin_dir,
        &mut config.home_dir,
        &mut config.patch_dir,
        &mut config.snapshot_dir,
    ]
    .iter_mut()
    {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else {
            config_dir.join(&path)
        };

        // Create directory and all parent directories
        std::fs::create_dir_all(&abs_path)
            .with_context(|| format!("Failed to create directory: {:?}", abs_path))?;

        // Now we can safely canonicalize
        **path = abs_path
            .canonicalize()
            .with_context(|| format!("Failed to resolve path: {:?}", abs_path))?;
    }

    debug!("Using app configuration\n{:?}", config);
    Ok(config)
}
