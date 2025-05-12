use anyhow::{Context, Result};
use log::{debug, info};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::config::GlobalConfig;

pub struct Builder {
    config: GlobalConfig,
    patches: Vec<String>,
}

impl Builder {
    pub fn new(config: GlobalConfig) -> Result<Self> {
        if !config.bench.global.source.exists() {
            anyhow::bail!(
                "Source directory does not exist: {}",
                config.bench.global.source.display()
            );
        }
        let patches = vec!["0001-validation-assumeutxo-benchmarking-patches.patch".to_string()];
        Ok(Self { config, patches })
    }

    fn binary_exists(&self, commit: &str) -> bool {
        let binary_path = self.config.app.bin_dir.join(format!("bitcoind-{}", commit));
        if binary_path.exists() {
            info!(
                "Binary already exists for commit {}, skipping build",
                commit
            );
            true
        } else {
            false
        }
    }

    pub fn build(&self) -> Result<()> {
        self.check_clean_worktree()?;
        let initial_ref = self.get_initial_ref()?;

        for commit in &self.config.bench.global.commits {
            if !self.binary_exists(commit) {
                info!("Building binary for commit {}", commit);
                self.build_commit(commit)?;
            }
        }

        self.restore_git_state(&initial_ref)?;
        Ok(())
    }

    fn check_clean_worktree(&self) -> Result<()> {
        // Check for unstaged changes
        let unstaged = Command::new("git")
            .current_dir(&self.config.bench.global.source)
            .args(["diff", "--quiet"])
            .status()?;

        if !unstaged.success() {
            anyhow::bail!("Worktree has unstaged changes. Please commit or stash them first.");
        }

        // Check for staged changes
        let staged = Command::new("git")
            .current_dir(&self.config.bench.global.source)
            .args(["diff", "--quiet", "--staged"])
            .status()?;

        if !staged.success() {
            anyhow::bail!("Worktree has staged changes. Please commit or stash them first.");
        }
        Ok(())
    }

    fn get_initial_ref(&self) -> Result<String> {
        let output = Command::new("git")
            .current_dir(&self.config.bench.global.source)
            .arg("symbolic-ref")
            .arg("-q")
            .arg("HEAD")
            .output()?;

        if output.status.success() {
            Ok(String::from_utf8(output.stdout)?.trim().to_string())
        } else {
            let output = Command::new("git")
                .current_dir(&self.config.bench.global.source)
                .arg("rev-parse")
                .arg("HEAD")
                .output()?;

            if output.status.success() {
                Ok(String::from_utf8(output.stdout)?.trim().to_string())
            } else {
                anyhow::bail!("Failed to get git ref");
            }
        }
    }

    fn build_commit(&self, original_commit: &str) -> Result<()> {
        self.checkout_commit(original_commit)?;
        let patched_commit = self.apply_patches()?;
        debug!("Commit hash after applying patches: {}", patched_commit);
        self.run_build(original_commit)?;
        self.copy_binary(original_commit)?;
        Ok(())
    }

    pub fn test_patch_commits(&self) -> Result<()> {
        self.check_clean_worktree()?;
        let initial_ref = self.get_initial_ref()?;
        for commit in &self.config.bench.global.commits {
            self.checkout_commit(commit)?;
            self.test_patches()?;
        }
        self.restore_git_state(&initial_ref)?;
        Ok(())
    }

    fn checkout_commit(&self, commit: &str) -> Result<()> {
        let status = Command::new("git")
            .current_dir(&self.config.bench.global.source)
            .arg("checkout")
            .arg(commit)
            .status()
            .with_context(|| format!("Failed to checkout commit {}", commit))?;

        if !status.success() {
            anyhow::bail!("Git checkout failed for commit {}", commit);
        }
        Ok(())
    }

    fn apply_patches(&self) -> Result<String> {
        self.process_patches(false)?;

        // Get the current commit hash after applying patches
        let output = Command::new("git")
            .current_dir(&self.config.bench.global.source)
            .arg("rev-parse")
            .arg("HEAD")
            .output()
            .context("Failed to get HEAD commit hash after applying patches")?;

        if !output.status.success() {
            anyhow::bail!("Failed to get HEAD commit hash after applying patches");
        }

        Ok(String::from_utf8(output.stdout)?.trim().to_string())
    }

    fn test_patches(&self) -> Result<()> {
        self.process_patches(true)
    }

    fn download_patch(&self, patch_name: &str, patches_dir: &PathBuf) -> Result<()> {
        let client = reqwest::blocking::Client::new();
        let url = format!(
            "https://raw.githubusercontent.com/bitcoin-dev-tools/benchkit/master/patches/{}",
            patch_name
        );
        let response = client.get(&url).send()?;

        if !response.status().is_success() {
            anyhow::bail!(
                "Failed to download patch {}: {}",
                patch_name,
                response.status()
            );
        }

        let content = response.bytes()?;
        let patch_path = patches_dir.join(patch_name);

        // Ensure the patches directory exists
        if !patches_dir.exists() {
            std::fs::create_dir_all(patches_dir)?;
        }

        std::fs::write(&patch_path, content)?;
        info!("Successfully downloaded patch: {}", patch_name);
        Ok(())
    }

    pub fn update_patches(&self, force: bool) -> Result<()> {
        for patch in &self.patches {
            let patch_path = &self.config.app.patch_dir.join(patch);
            if !patch_path.exists() || force {
                info!("Downloading patch: {patch}");
                self.download_patch(patch, &self.config.app.patch_dir)?;
            } else {
                info!("Patch {patch} already exists, skipping download");
            }
        }
        Ok(())
    }

    fn process_patches(&self, check_only: bool) -> Result<()> {
        self.update_patches(false)?;

        let patches_dir = &self.config.app.patch_dir;

        // Verify all patches exist
        for patch in &self.patches {
            let patch_path = patches_dir.join(patch);
            if !patch_path.exists() {
                anyhow::bail!("Patch file not found: {}", patch_path.display());
            }
        }

        // Apply each patch
        for patch in &self.patches {
            let patch_path = patches_dir.join(patch);
            let operation = if check_only { "Testing" } else { "Applying" };
            info!("{} patch: {}", operation, patch_path.display());

            let mut cmd = Command::new("git");
            cmd.current_dir(&self.config.bench.global.source);

            if check_only {
                cmd.arg("apply")
                    .arg("--check")
                    .arg(patch_path.display().to_string());
            } else {
                cmd.arg("-c")
                    .arg("user.name=temp")
                    .arg("-c")
                    .arg("user.email=temp@temp.com")
                    .arg("am")
                    .arg("--no-signoff")
                    .arg(patch_path.display().to_string());
            }

            let status = cmd.status().with_context(|| {
                let action = if check_only { "test" } else { "apply" };
                format!("Failed to {} patch {}", action, patch)
            })?;

            if !status.success() {
                if !check_only {
                    // If patch application fails, abort the am session
                    let _ = Command::new("git")
                        .current_dir(&self.config.bench.global.source)
                        .arg("am")
                        .arg("--abort")
                        .status();
                }
                anyhow::bail!(
                    "Failed to {} patch: {}",
                    if check_only { "test" } else { "apply" },
                    patch
                );
            }

            let action = if check_only { "tested" } else { "applied" };
            info!("Successfully {} patch: {}", action, patch);
        }
        Ok(())
    }

    fn run_build(&self, original_commit: &str) -> Result<()> {
        // Make a build-dir using the short commit-hash
        let dir = self
            .config
            .bench
            .global
            .scratch
            .join(format!("build-{}", original_commit));

        info!("Making build dir: {:?}", dir);
        fs::create_dir(&dir)?;
        let canonical_dir = fs::canonicalize(&dir)?;

        // Run cmake configuration
        let mut cmd = Command::new("cmake");
        cmd.current_dir(&self.config.bench.global.source)
            .arg("-B")
            .arg(&canonical_dir)
            .arg("-DBUILD_CLI=OFF")
            .arg("-DBUILD_TESTS=OFF")
            .arg("-DCMAKE_CXX_FLAGS=-fno-omit-frame-pointer");

        let config_status = cmd
            .status()
            .with_context(|| format!("Failed to configure cmake for commit {}", original_commit))?;

        if !config_status.success() {
            anyhow::bail!("CMake configuration failed for commit {}", original_commit);
        }

        // Run cmake build
        let mut cmake_build = if std::env::var("CI").is_ok() {
            let mut cmd = Command::new("taskset");
            cmd.current_dir(&self.config.bench.global.source)
                .arg("-c")
                .arg("2-15")
                .arg("chrt")
                .arg("-f")
                .arg("1")
                .arg("cmake")
                .arg("--build")
                .arg(&canonical_dir)
                .arg("--target")
                .arg("bitcoind")
                .arg("--parallel");
            cmd
        } else {
            let mut cmd = Command::new("cmake");
            cmd.current_dir(&self.config.bench.global.source)
                .arg("--build")
                .arg(&canonical_dir)
                .arg("--target")
                .arg("bitcoind")
                .arg("--parallel");
            cmd
        };

        let build_status = cmake_build
            .status()
            .with_context(|| format!("Failed to build bitcoind for commit {}", original_commit))?;

        if !build_status.success() {
            anyhow::bail!("CMake build failed for commit {}", original_commit);
        }

        Ok(())
    }

    fn copy_binary(&self, original_commit: &str) -> Result<()> {
        let dir = self
            .config
            .bench
            .global
            .scratch
            .join(format!("build-{}", original_commit));
        let src_path = dir.clone().join("bin/bitcoind");
        let dest_path = self
            .config
            .app
            .bin_dir
            .join(format!("bitcoind-{}", original_commit));
        debug!("Copying {src_path:?} to {dest_path:?}");

        std::fs::copy(&src_path, &dest_path)
            .with_context(|| format!("Failed to copy binary for commit {}", original_commit))?;

        std::fs::remove_dir_all(
            self.config
                .bench
                .global
                .scratch
                .join(format!("build-{}", original_commit)),
        )
        .with_context(|| {
            format!(
                "Failed to cleanup extracted files for commit {} from {:?}",
                original_commit, dir
            )
        })?;

        Ok(())
    }

    fn restore_git_state(&self, initial_ref: &str) -> Result<()> {
        let status = Command::new("git")
            .current_dir(&self.config.bench.global.source)
            .arg("checkout")
            .arg(initial_ref)
            .status()
            .with_context(|| format!("Failed to restore git state to {}", initial_ref))?;

        if !status.success() {
            anyhow::bail!("Failed to restore git state");
        }
        Ok(())
    }
}
