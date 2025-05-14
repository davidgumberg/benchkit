use std::path::PathBuf;

/// Common trait for configuration types
pub trait Configuration {
    /// Returns the file path where this configuration was loaded from
    fn config_path(&self) -> &PathBuf;

    /// Returns a string identifier for the configuration type
    fn config_type(&self) -> &str;

    /// Validates the configuration
    fn validate(&self) -> anyhow::Result<()>;
}

/// Trait for configurations with path elements that need expansion
pub trait PathConfiguration: Configuration {
    /// Returns a list of path fields that need expansion
    fn paths_for_expansion(&self) -> Vec<&PathBuf>;

    /// Returns a copy with expanded paths
    fn with_expanded_paths(&self, config_dir: &std::path::Path) -> anyhow::Result<Self>
    where
        Self: Sized;
}

/// Trait for configurations that can be merged together
pub trait MergeableConfiguration<T> {
    /// Merges this configuration with another, with the other taking precedence
    fn merge_with(&self, other: &T) -> anyhow::Result<Self>
    where
        Self: Sized;
}
