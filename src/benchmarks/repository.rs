use anyhow::{Context, Result};
use log::{debug, info, warn};
use std::path::{Path, PathBuf};
use std::process::Command;
use url::Url;

/// Repository source can be either local or remote
#[derive(Debug, Clone)]
pub enum RepoSource {
    /// Local directory path
    Local(PathBuf),
    /// Remote Git URL
    Remote(String),
}

impl RepoSource {
    /// Create a new RepoSource from a source string
    /// This will detect if the source is a URL or a local path
    pub fn new(source: &str) -> Self {
        if let Ok(url) = Url::parse(source) {
            if url.scheme() == "http" || url.scheme() == "https" || url.scheme() == "git" {
                return RepoSource::Remote(source.to_string());
            }
        }
        if source.starts_with("git@") && source.contains(':') && source.ends_with(".git") {
            return RepoSource::Remote(source.to_string());
        }

        // Otherwise, treat as local path
        RepoSource::Local(PathBuf::from(source))
    }

    /// Get a clean directory name for the repository
    /// This is used for creating cache directories
    pub fn get_cache_name(&self) -> String {
        match self {
            RepoSource::Local(path) => {
                // For local repos, use the directory name
                path.file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| "local_repo".to_string())
            }
            RepoSource::Remote(url) => {
                // For remote repos, sanitize the URL to create a valid directory name
                // Remove protocol and special characters
                let url_str = url.to_string();
                url_str
                    .replace("http://", "")
                    .replace("https://", "")
                    .replace("git://", "")
                    .replace("git@", "")
                    .replace([':', '/', '.'], "_")
            }
        }
    }
}

/// Builder for RepositoryManager
pub struct RepositoryManagerBuilder {
    /// The source of the repository (local or remote)
    source: RepoSource,
    /// Directory to store cloned repositories
    cache_dir: PathBuf,
    /// Custom repository name for caching
    custom_repo_name: Option<String>,
    /// Skip validation of repository structure
    skip_validation: bool,
}

impl RepositoryManagerBuilder {
    /// Create a new RepositoryManagerBuilder with required parameters
    pub fn new(source: &str, scratch_dir: &Path) -> Self {
        let repo_source = RepoSource::new(source);

        // Create a repos directory in scratch_dir if not already there
        let cache_dir = if scratch_dir.ends_with("repos") {
            scratch_dir.to_path_buf()
        } else {
            scratch_dir.join("repos")
        };

        Self {
            source: repo_source,
            cache_dir,
            custom_repo_name: None,
            skip_validation: false,
        }
    }

    /// Set a custom repository name for caching
    /// This overrides the auto-generated name from the source
    pub fn custom_repo_name(mut self, name: impl Into<String>) -> Self {
        self.custom_repo_name = Some(name.into());
        self
    }

    /// Set the cache directory to use
    pub fn cache_dir(mut self, dir: impl AsRef<Path>) -> Self {
        self.cache_dir = dir.as_ref().to_path_buf();
        self
    }

    /// Skip validation of repository structure
    pub fn skip_validation(mut self, skip: bool) -> Self {
        self.skip_validation = skip;
        self
    }

    /// Build the RepositoryManager
    pub fn build(self) -> Result<RepositoryManager> {
        // Create the cache directory if it doesn't exist
        if !self.cache_dir.exists() {
            std::fs::create_dir_all(&self.cache_dir)?;
        }

        debug!("Repository cache directory: {}", self.cache_dir.display());

        // If we're not skipping validation, do some checks based on the source type
        if !self.skip_validation {
            match &self.source {
                RepoSource::Local(path) => {
                    // Check that the local path exists
                    if !path.exists() {
                        return Err(anyhow::anyhow!(
                            "Local repository path does not exist: {}",
                            path.display()
                        ));
                    }
                }
                RepoSource::Remote(url) => {
                    // Minimal URL validation (a more comprehensive check would try to parse the URL)
                    if !(url.starts_with("http://")
                        || url.starts_with("https://")
                        || url.starts_with("git://")
                        || url.starts_with("git@"))
                    {
                        return Err(anyhow::anyhow!("Invalid Git URL format: {}", url));
                    }
                }
            }
        }

        Ok(RepositoryManager {
            source: self.source,
            cache_dir: self.cache_dir,
            repo_path: None,
            custom_repo_name: self.custom_repo_name,
        })
    }
}

/// Manages Git repositories for benchmarking
pub struct RepositoryManager {
    /// The source of the repository (local or remote)
    source: RepoSource,
    /// Directory to store cloned repositories
    cache_dir: PathBuf,
    /// Path to the actual repository to use (either original local path or cloned path)
    repo_path: Option<PathBuf>,
    /// Custom repository name for caching
    custom_repo_name: Option<String>,
}

impl RepositoryManager {
    /// Create a new repository manager
    pub fn new(source: &str, scratch_dir: &Path) -> Self {
        let repo_source = RepoSource::new(source);

        // Create a repos directory in scratch_dir if not already there
        let cache_dir = if scratch_dir.ends_with("repos") {
            scratch_dir.to_path_buf()
        } else {
            scratch_dir.join("repos")
        };

        debug!("Repository cache directory: {}", cache_dir.display());

        Self {
            source: repo_source,
            cache_dir,
            repo_path: None,
            custom_repo_name: None,
        }
    }

    /// Create a new RepositoryManagerBuilder
    pub fn builder(source: &str, scratch_dir: &Path) -> RepositoryManagerBuilder {
        RepositoryManagerBuilder::new(source, scratch_dir)
    }

    /// Ensure the repository is available locally
    /// If it's a remote repository, clone it if needed
    /// Returns the path to the repository to use for operations
    pub fn ensure_repository_available(&mut self) -> Result<PathBuf> {
        match &self.source {
            RepoSource::Local(path) => {
                // For local repos, just check if it exists and contains a .git directory
                if !path.exists() {
                    anyhow::bail!("Source directory does not exist: {}", path.display());
                }

                let git_dir = path.join(".git");
                if !git_dir.exists() {
                    anyhow::bail!(
                        "Source directory is not a Git repository: {}",
                        path.display()
                    );
                }

                self.repo_path = Some(path.clone());
                Ok(path.clone())
            }
            RepoSource::Remote(url) => {
                // For remote repos, check if we have it cached already
                let repo_name = self
                    .custom_repo_name
                    .clone()
                    .unwrap_or_else(|| self.source.get_cache_name());
                let repo_path = self.cache_dir.join(&repo_name);

                if repo_path.exists() {
                    debug!("Using cached repository: {}", repo_path.display());
                    // Repository already exists, just update it
                    self.update_repository(&repo_path)?;
                } else {
                    // Repository doesn't exist, clone it
                    info!("Cloning repository: {} to {}", url, repo_path.display());
                    self.clone_repository(url, &repo_path)?;
                }

                self.repo_path = Some(repo_path.clone());
                Ok(repo_path)
            }
        }
    }

    /// Clone a repository
    fn clone_repository(&self, url: &str, target_path: &Path) -> Result<()> {
        // Create parent directory if needed
        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let status = Command::new("git")
            .arg("clone")
            .arg(url)
            .arg(target_path)
            .status()
            .context(format!("Failed to clone repository: {}", url))?;

        if !status.success() {
            anyhow::bail!("Git clone failed with status code: {}", status);
        }

        Ok(())
    }

    /// Update an existing repository
    fn update_repository(&self, repo_path: &Path) -> Result<()> {
        // Fetch latest changes
        let status = Command::new("git")
            .current_dir(repo_path)
            .arg("fetch")
            .status()
            .context(format!(
                "Failed to update repository: {}",
                repo_path.display()
            ))?;

        if !status.success() {
            warn!("Git fetch failed with status code: {}", status);
            // Continue anyway, as the repository might still be usable
        }

        Ok(())
    }

    /// Get the path to the repository
    pub fn get_repository_path(&self) -> Result<PathBuf> {
        self.repo_path.clone().ok_or_else(|| {
            anyhow::anyhow!("Repository not initialized. Call ensure_repository_available() first.")
        })
    }

    /// Validate that all required commits are available in the repository
    pub fn validate_commits(&self, commits: &[String]) -> Result<()> {
        let repo_path = self.repo_path.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Repository not initialized. Call ensure_repository_available() first.")
        })?;

        for commit in commits {
            // Try to get commit info to verify it exists
            let output = Command::new("git")
                .current_dir(repo_path)
                .arg("cat-file")
                .arg("-t")
                .arg(commit)
                .output()
                .context(format!("Failed to check commit: {}", commit))?;

            if !output.status.success() {
                anyhow::bail!("Commit not found in repository: {}", commit);
            }

            // Verify it's a commit object
            let obj_type = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if obj_type != "commit" {
                anyhow::bail!("Object is not a commit: {} (type: {})", commit, obj_type);
            }
        }

        Ok(())
    }
}
