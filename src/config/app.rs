use anyhow::{Context, Result};
use log::debug;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};
use tempfile::TempDir;

use crate::config::{NetConfig, RawNetConfig};
use crate::path_utils::process_path;

/// Enum to hold user-selected tmpdir vs our own.
#[derive(Debug, Clone)]
pub enum TmpDataDir {
    Persistent(PathBuf),
    Temporary(Arc<TempDir>),
}

impl TmpDataDir {
    pub fn path(&self) -> &std::path::Path {
        match self {
            Self::Persistent(p) => p,
            Self::Temporary(t) => t.path(),
        }
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
    /// number of cores to use in building.
    pub build_cores: Option<usize>,
    pub net: Option<RawNetConfig>,
}

impl RawAppConfig {
    /// Convert a serialization-only `RawAppConfig` into a real one, avoid
    /// business logic here, just path expansion / parsing stuff, everything
    /// else goes in AppConfig::new()
    pub fn finalize(self, path: &PathBuf) -> Result<AppConfig> {
        let config_dir = path
            .parent()
            .context("Failed to get app config directory")?;

        let scratch_dir = process_path(&self.scratch_dir, config_dir, true)?;

        // Expand the paths (only if they were passed by the user), otherwise
        let bin_dir = self.bin_dir
            .map(|p| process_path(&p, config_dir, true))
            .transpose()?;

        let tmp_datadir = self.tmp_datadir
            .map(|p| process_path(&p, config_dir, true))
            .transpose()?;
        let net = self.net
            .map(|raw| raw.finalize(config_dir))
            .transpose()?;

        AppConfig::new(
            path.to_path_buf(),
            scratch_dir,
            bin_dir,
            tmp_datadir,
            self.build_cores,
            net,
        )
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct AppConfig {
    #[serde(default)]
    pub path: PathBuf,
    pub scratch_dir: PathBuf,
    pub bin_dir: PathBuf,
    pub tmp_datadir: TmpDataDir,
    pub build_cores: Option<usize>,
    pub net: Option<NetConfig>,
}

impl AppConfig {
    pub fn new(path: PathBuf, scratch_dir: PathBuf, bin_dir: Option<PathBuf>, tmp_datadir: Option<PathBuf>, build_cores: Option<usize>, net: Option<NetConfig>) -> Result<Self> {
        let bin_dir = bin_dir.unwrap_or_else(|| scratch_dir.join("binaries"));
        let tmp_datadir = match tmp_datadir {
            Some(p) => TmpDataDir::Persistent(p),
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
            build_cores,
            net
        })
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        let path = PathBuf::default();
        let scratch_dir = PathBuf::default();
        let bin_dir = PathBuf::default();
        let tmp_datadir = TmpDataDir::Persistent(PathBuf::default());
        let build_cores = Option::default();
        let net = Option::default();

        Self {
            path,
            scratch_dir,
            bin_dir,
            tmp_datadir,
            build_cores,
            net
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::tempdir;
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
