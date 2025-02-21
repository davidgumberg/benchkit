use anyhow::{Context, Result};
use log::{debug, info, warn};
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

    fn build_commit(&self, commit: &str) -> Result<()> {
        self.checkout_commit(commit)?;
        self.apply_patches()?;
        self.run_build(commit)?;
        self.copy_binary(commit)?;
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

    fn apply_patches(&self) -> Result<()> {
        let patches = ["assumeutxo.patch", "guix.patch"];

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

            // First try with -3 for git-apply to attempt 3-way merge
            let status = Command::new("git")
                .current_dir(&self.config.bench.global.source)
                .arg("apply")
                .arg("-3")
                .arg("--whitespace=fix")
                .arg(patch_path.display().to_string())
                .status()
                .with_context(|| format!("Failed to execute git apply for patch {}", patch))?;

            if !status.success() {
                // If 3-way merge fails, try to apply patch with --reject
                warn!("3-way merge failed for {}, attempting with --reject", patch);

                let status = Command::new("git")
                    .current_dir(&self.config.bench.global.source)
                    .arg("apply")
                    .arg("--reject")
                    .arg("--whitespace=fix")
                    .arg(patch_path.display().to_string())
                    .status()
                    .with_context(|| {
                        format!("Failed to execute git apply --reject for patch {}", patch)
                    })?;

                if !status.success() {
                    anyhow::bail!("Failed to apply patch {} even with --reject", patch);
                }

                // Check for .rej files which indicate partial application
                let output = Command::new("find")
                    .current_dir(&self.config.bench.global.source)
                    .arg(".")
                    .arg("-name")
                    .arg("*.rej")
                    .output()
                    .with_context(|| "Failed to search for .rej files")?;

                if !output.stdout.is_empty() {
                    let rej_files = String::from_utf8_lossy(&output.stdout);
                    anyhow::bail!(
                        "Patch {} was only partially applied. Review these .rej files:\n{}",
                        patch,
                        rej_files
                    );
                }
            }

            info!("Successfully applied patch: {}", patch);
        }

        Ok(())
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

    fn run_build(&self, commit: &str) -> Result<()> {
        let short_commit = self.get_full_commit(commit).unwrap();

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
        cmd.env("FORCE_DIRTY_WORKTREE", "1");

        // Conditionally set environment variables if they exist
        let env_vars = ["HOSTS", "SOURCES_PATH", "BASE_CACHE", "SDK_PATH"];
        for var in &env_vars {
            if let Ok(value) = std::env::var(var) {
                cmd.env(var, value);
            }
        }

        let status = cmd
            .status()
            .with_context(|| format!("Failed to run build for commit {}", commit))?;

        if !status.success() {
            anyhow::bail!("Build failed for commit {}", commit);
        }

        let archive_path = self.config.bench.global.source.join(format!(
            "guix-build-{}/output/x86_64-linux-gnu/bitcoin-{}-x86_64-linux-gnu.tar.gz",
            short_commit, short_commit
        ));

        let status = Command::new("tar")
            .current_dir(&self.config.bench.global.source)
            .arg("-xzf")
            .arg(&archive_path)
            .status()
            .with_context(|| format!("Failed to extract archive for commit {}", commit))?;

        if !status.success() {
            anyhow::bail!("Failed to extract archive for commit {}", commit);
        }

        let status = Command::new("git")
            .current_dir(&self.config.bench.global.source)
            .arg("reset")
            .arg("--hard")
            .status()
            .with_context(|| "Failed to reset uncommited patches after build")?;

        if !status.success() {
            anyhow::bail!("Git restore failed.",);
        }

        Ok(())
    }

    fn copy_binary(&self, commit: &str) -> Result<()> {
        let short_commit = &self.get_full_commit(commit).unwrap()[..12];
        let src_path = self
            .config
            .bench
            .global
            .source
            .join(format!("bitcoin-{}/bin/bitcoind", short_commit));
        let dest_path = self.config.app.bin_dir.join(format!("bitcoind-{}", commit));

        std::fs::copy(&src_path, &dest_path)
            .with_context(|| format!("Failed to copy binary for commit {}", commit))?;

        std::fs::remove_dir_all(
            self.config
                .bench
                .global
                .source
                .join(format!("bitcoin-{}", short_commit)),
        )
        .with_context(|| format!("Failed to cleanup extracted files for commit {}", commit))?;

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
