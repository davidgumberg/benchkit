use anyhow::{Context, Result};
use log::debug;
use serde::Deserialize;
use std::path::PathBuf;

use crate::config::traits::{Configuration, PathConfiguration};
use crate::path_utils;

/// Application configuration loaded from config.yml
#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    /// Directory for storing built binaries
    pub bin_dir: PathBuf,
    /// Home directory for configuration and data
    pub home_dir: PathBuf,
    /// Directory containing patch files
    pub patch_dir: PathBuf,
    /// Directory for snapshot files
    pub snapshot_dir: PathBuf,
    /// Path to the config file (set during loading)
    #[serde(default)]
    pub path: PathBuf,
}

impl Configuration for AppConfig {
    fn config_path(&self) -> &PathBuf {
        &self.path
    }

    fn config_type(&self) -> &str {
        "application"
    }

    fn validate(&self) -> anyhow::Result<()> {
        // Validate directory paths exist or can be created
        if !self.bin_dir.exists() && std::fs::create_dir_all(&self.bin_dir).is_err() {
            anyhow::bail!("Invalid bin_dir path: {}", self.bin_dir.display());
        }

        if !self.patch_dir.exists() && std::fs::create_dir_all(&self.patch_dir).is_err() {
            anyhow::bail!("Invalid patch_dir path: {}", self.patch_dir.display());
        }

        if !self.snapshot_dir.exists() && std::fs::create_dir_all(&self.snapshot_dir).is_err() {
            anyhow::bail!("Invalid snapshot_dir path: {}", self.snapshot_dir.display());
        }

        Ok(())
    }
}

impl PathConfiguration for AppConfig {
    fn paths_for_expansion(&self) -> Vec<&PathBuf> {
        vec![
            &self.bin_dir,
            &self.home_dir,
            &self.patch_dir,
            &self.snapshot_dir,
        ]
    }

    fn with_expanded_paths(&self, config_dir: &std::path::Path) -> anyhow::Result<Self> {
        let mut config = self.clone();
        path_utils::process_paths(
            &mut [
                &mut config.bin_dir,
                &mut config.home_dir,
                &mut config.patch_dir,
                &mut config.snapshot_dir,
            ],
            config_dir,
            true,
        )?;
        Ok(config)
    }
}

/// Load application configuration from a YAML file
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

    // Set the configuration path
    config.path = app_config_path.to_path_buf();

    // Use the PathConfiguration trait to expand paths
    let config = config.with_expanded_paths(config_dir)?;

    // Validate the configuration
    config.validate()?;

    debug!("Using {} configuration\n{:?}", config.config_type(), config);
    Ok(config)
}
