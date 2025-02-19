use anyhow::{Context, Result};
use std::env;
use std::path::{Path, PathBuf};

/// Configuration for benchkit's home directory and related paths
#[derive(Debug, Clone)]
pub struct HomeConfig {
    /// Root directory for benchkit data
    pub home_dir: PathBuf,
    /// Directory for storing built binaries
    pub binaries_dir: PathBuf,
}

impl Default for HomeConfig {
    fn default() -> Self {
        // Default to $HOME/.local/state/benchkit
        let home = env::var("HOME").expect("HOME environment variable not set");
        let home_dir = PathBuf::from(home).join(".local/state/benchkit");

        Self::new(home_dir)
    }
}

impl HomeConfig {
    /// Create a new HomeConfig with the specified home directory
    pub fn new(home_dir: PathBuf) -> Self {
        let binaries_dir = home_dir.join("binaries");

        Self {
            home_dir,
            binaries_dir,
        }
    }

    /// Initialize the home directory structure
    pub fn initialize(&self) -> Result<()> {
        // Create home directory if it doesn't exist
        if !self.home_dir.exists() {
            std::fs::create_dir_all(&self.home_dir).with_context(|| {
                format!("Failed to create home directory at {:?}", self.home_dir)
            })?;
        }

        // Create binaries directory if it doesn't exist
        if !self.binaries_dir.exists() {
            std::fs::create_dir_all(&self.binaries_dir).with_context(|| {
                format!(
                    "Failed to create binaries directory at {:?}",
                    self.binaries_dir
                )
            })?;
        }

        Ok(())
    }

    /// Get home directory from environment or use default
    pub fn from_env() -> Self {
        if let Ok(dir) = env::var("BENCHCOIN_HOME") {
            Self::new(PathBuf::from(dir))
        } else {
            Self::default()
        }
    }

    /// Get home directory from command line option, environment, or default
    pub fn from_option(home_dir: Option<&Path>) -> Self {
        if let Some(dir) = home_dir {
            Self::new(dir.to_path_buf())
        } else {
            Self::from_env()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::TempDir;

    #[test]
    fn test_default_home_config() {
        let home = env::var("HOME").unwrap();
        let config = HomeConfig::default();

        assert_eq!(
            config.home_dir,
            PathBuf::from(&home).join(".local/state/benchkit")
        );
        assert_eq!(
            config.binaries_dir,
            PathBuf::from(&home).join(".local/state/benchkit/binaries")
        );
    }

    #[test]
    fn test_custom_home_config() {
        let temp_dir = TempDir::new().unwrap();
        let config = HomeConfig::new(temp_dir.path().to_path_buf());

        assert_eq!(config.home_dir, temp_dir.path());
        assert_eq!(config.binaries_dir, temp_dir.path().join("binaries"));
    }

    #[test]
    fn test_initialize_creates_directories() {
        let temp_dir = TempDir::new().unwrap();
        let config = HomeConfig::new(temp_dir.path().to_path_buf());

        config.initialize().unwrap();

        assert!(config.home_dir.exists());
        assert!(config.binaries_dir.exists());
    }

    #[test]
    fn test_from_env() {
        let temp_dir = TempDir::new().unwrap();
        env::set_var("BENCHCOIN_HOME", temp_dir.path());

        let config = HomeConfig::from_env();

        assert_eq!(config.home_dir, temp_dir.path());
        assert_eq!(config.binaries_dir, temp_dir.path().join("binaries"));

        env::remove_var("BENCHCOIN_HOME");
    }

    #[test]
    fn test_from_option() {
        let temp_dir = TempDir::new().unwrap();

        // Test with Some path
        let config = HomeConfig::from_option(Some(temp_dir.path()));
        assert_eq!(config.home_dir, temp_dir.path());

        // Test with None (should use default)
        let config = HomeConfig::from_option(None);
        let home = env::var("HOME").unwrap();
        assert_eq!(
            config.home_dir,
            PathBuf::from(home).join(".local/state/benchkit")
        );
    }
}
