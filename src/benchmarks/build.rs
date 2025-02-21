use anyhow::{Context, Result};
use log::{debug, info};
use std::process::Command;

use crate::config::GlobalConfig;

pub struct Builder {
    config: GlobalConfig,
}

impl Builder {
    pub fn new(config: GlobalConfig) -> Result<Self> {
        if !config.bench.global.source.exists() {
            anyhow::bail!(
                "Source directory does not exist: {}",
                config.bench.global.source.display()
            );
        }
        Ok(Self { config })
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
        self.run_build(patched_commit.as_str())?;
        self.copy_binary(patched_commit.as_str(), original_commit)?;
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
        let patches = [
            "0001-guix-benchmarking-patches.patch",
            "0001-validation-assumeutxo-benchmarking-patches.patch",
        ];

        // Get the absolute path to the patches directory
        let patches_dir = std::env::current_dir()?.join("patches");

        // First verify all patches exist
        for patch in &patches {
            let patch_path = patches_dir.join(patch);
            if !patch_path.exists() {
                anyhow::bail!("Patch file not found: {}", patch_path.display());
            }
        }

        // Apply each patch
        for patch in &patches {
            let patch_path = patches_dir.join(patch);
            info!("Applying patch: {}", patch_path.display());

            let status = Command::new("git")
                .current_dir(&self.config.bench.global.source)
                .arg("-c")
                .arg("user.name=temp")
                .arg("-c")
                .arg("user.email=temp@temp.com")
                .arg("am")
                .arg("--no-signoff")
                .arg(patch_path.display().to_string())
                .status()
                .with_context(|| format!("Failed to execute git am for patch {}", patch))?;

            if !status.success() {
                // If patch application fails, abort the am session
                let _ = Command::new("git")
                    .current_dir(&self.config.bench.global.source)
                    .arg("am")
                    .arg("--abort")
                    .status();

                anyhow::bail!("Failed to apply patch {}", patch);
            }

            info!("Successfully applied patch: {}", patch);
        }

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

    fn get_full_commit(&self, commit: &str) -> Result<String> {
        let output = Command::new("git")
            .current_dir(&self.config.bench.global.source)
            .arg("rev-parse")
            .arg(commit)
            .output()
            .with_context(|| format!("Failed to expand commit hash '{}'", commit))?;

        if !output.status.success() {
            anyhow::bail!("Failed to resolve commit hash '{}'", commit);
        }

        let full_hash = String::from_utf8(output.stdout)
            .with_context(|| format!("Invalid UTF-8 in git output for commit '{}'", commit))?
            .trim()
            .to_string();
        debug!(
            "Resolved config commit {} to full hash {}",
            commit, full_hash
        );
        Ok(String::from(&full_hash[..12]))
    }

    fn run_build(&self, patched_commit: &str) -> Result<()> {
        let short_patched_commit = self.get_full_commit(patched_commit).unwrap();

        // Create base command depending on CI environment
        let mut cmd = if std::env::var("CI").is_ok() {
            let mut cmd = Command::new("taskset");
            cmd.current_dir(&self.config.bench.global.source)
                .arg("-c")
                .arg("2-15")
                .arg("chrt")
                .arg("-f")
                .arg("1")
                .arg("contrib/guix/guix-build");
            cmd
        } else {
            let mut cmd = Command::new("contrib/guix/guix-build");
            cmd.current_dir(&self.config.bench.global.source);
            cmd
        };

        // Always set this as we apply patches but we don't want to commit
        // cmd.env("FORCE_DIRTY_WORKTREE", "1");

        // Conditionally set some guix environment variables if they exist
        let env_vars = ["SOURCES_PATH", "BASE_CACHE", "SDK_PATH"];
        for var in &env_vars {
            if let Ok(value) = std::env::var(var) {
                cmd.env(var, value);
            }
        }
        cmd.env("HOSTS", self.config.bench.global.host.clone());

        let status = cmd
            .status()
            .with_context(|| format!("Failed to run build for commit {}", patched_commit))?;

        if !status.success() {
            anyhow::bail!("Build failed for commit {}", patched_commit);
        }

        let archive_path = self.config.bench.global.source.join(format!(
            "guix-build-{}/output/{}/bitcoin-{}-{}.tar.gz",
            short_patched_commit,
            self.config.bench.global.host,
            short_patched_commit,
            self.config.bench.global.host,
        ));

        let status = Command::new("tar")
            .current_dir(&self.config.bench.global.source)
            .arg("-xzf")
            .arg(&archive_path)
            .status()
            .with_context(|| format!("Failed to extract archive for commit {}", patched_commit))?;

        if !status.success() {
            anyhow::bail!("Failed to extract archive for commit {}", patched_commit);
        }

        Ok(())
    }

    fn copy_binary(&self, patched_commit: &str, original_commit: &str) -> Result<()> {
        let short_patched_commit = &self.get_full_commit(patched_commit).unwrap()[..12];
        // let short_original_commit = &self.get_full_commit(original_commit).unwrap()[..12];
        let src_path = self
            .config
            .bench
            .global
            .source
            .join(format!("bitcoin-{}/bin/bitcoind", short_patched_commit));
        let dest_path = self
            .config
            .app
            .bin_dir
            .join(format!("bitcoind-{}", original_commit));
        debug!("Copying {src_path:?} to {dest_path:?}");

        std::fs::copy(&src_path, &dest_path).with_context(|| {
            format!("Failed to copy binary for commit {}", short_patched_commit)
        })?;

        std::fs::remove_dir_all(
            self.config
                .bench
                .global
                .source
                .join(format!("bitcoin-{}", short_patched_commit)),
        )
        .with_context(|| {
            format!(
                "Failed to cleanup extracted files for commit {}",
                short_patched_commit
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
