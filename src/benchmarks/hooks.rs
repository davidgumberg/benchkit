use anyhow::{Context, Result};
use log::{debug, info};
use std::fs;
use std::path::Path;

use crate::benchmarks::hook_runner::HookArgs;
use crate::command::{CommandContext, CommandExecutor};

/// Different modes for benchmark hook execution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HookMode {
    /// AssumeUTXO mode - uses snapshot syncing
    #[default]
    AssumeUtxo,
    /// Full IBD mode - no snapshot, full initial block download
    FullIbd,
}

impl HookMode {
    /// Parse hook mode from string
    pub fn mode_from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "assumeutxo" => Ok(HookMode::AssumeUtxo),
            "full_ibd" | "fullibd" | "full-ibd" => Ok(HookMode::FullIbd),
            _ => anyhow::bail!("Unknown hook mode: {}", s),
        }
    }
}

/// Trait for executing benchmark lifecycle hooks
pub trait HookExecutor {
    fn setup(&self, args: &HookArgs) -> Result<()>;
    fn prepare(&self, args: &HookArgs) -> Result<()>;
    fn conclude(&self, args: &HookArgs) -> Result<()>;
    fn cleanup(&self, args: &HookArgs) -> Result<()>;
}

/// Base hook executor with common functionality
struct BaseHookExecutor;

impl BaseHookExecutor {
    /// Create a directory, including all parent directories
    fn create_directory(&self, path: &Path) -> Result<()> {
        debug!("Creating directory: {}", path.display());
        fs::create_dir_all(path)
            .with_context(|| format!("Failed to create directory: {}", path.display()))?;
        Ok(())
    }

    /// Clear all contents of a directory
    fn clear_directory(&self, path: &Path) -> Result<()> {
        debug!("Clearing directory contents: {}", path.display());

        if path.exists() {
            for entry in fs::read_dir(path)
                .with_context(|| format!("Failed to read directory: {}", path.display()))?
            {
                let entry = entry?;
                let entry_path = entry.path();

                if entry_path.is_dir() {
                    fs::remove_dir_all(&entry_path).with_context(|| {
                        format!("Failed to remove directory: {}", entry_path.display())
                    })?;
                } else {
                    fs::remove_file(&entry_path).with_context(|| {
                        format!("Failed to remove file: {}", entry_path.display())
                    })?;
                }
            }
        }

        Ok(())
    }

    /// Clear and recreate a directory
    fn clear_and_recreate_directory(&self, path: &Path) -> Result<()> {
        self.create_directory(path)?;
        self.clear_directory(path)?;
        Ok(())
    }
}

/// AssumeUTXO hook executor - uses snapshot syncing
pub struct AssumeUtxoHookExecutor {
    base: BaseHookExecutor,
}

impl AssumeUtxoHookExecutor {
    pub fn new() -> Self {
        Self {
            base: BaseHookExecutor,
        }
    }

    fn execute_bitcoin_command(&self, binary: &str, args: &[&str]) -> Result<()> {
        let command_desc = format!("{} {}", binary, args.join(" "));
        let context = CommandContext {
            command_name: Some(command_desc),
            allow_failure: false,
            capture_output: true,
            ..CommandContext::default()
        };

        let executor = CommandExecutor::with_context(context);
        let status = executor
            .execute_check_status(binary, args)
            .with_context(|| "Failed to execute Bitcoin Core command".to_string())?;

        if !status.success() {
            return Err(anyhow::anyhow!(
                "Bitcoin Core command failed with status {}",
                status.code().unwrap_or(-1)
            ));
        }

        Ok(())
    }

    /// Execute a Bitcoin Core command that may fail (like loadutxosnapshot)
    fn execute_bitcoin_command_allow_failure(&self, binary: &str, args: &[&str]) -> Result<()> {
        let command_desc = format!("{} {}", binary, args.join(" "));
        let context = CommandContext {
            command_name: Some(command_desc),
            allow_failure: true,
            capture_output: true,
            ..CommandContext::default()
        };

        let executor = CommandExecutor::with_context(context);
        let _ = executor.execute_check_status(binary, args);

        Ok(())
    }
}

impl Default for AssumeUtxoHookExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl HookExecutor for AssumeUtxoHookExecutor {
    fn setup(&self, args: &HookArgs) -> Result<()> {
        info!("Running AssumeUTXO setup hook");
        self.base.create_directory(&args.tmp_data_dir)?;
        self.base.clear_directory(&args.tmp_data_dir)?;

        Ok(())
    }

    fn prepare(&self, args: &HookArgs) -> Result<()> {
        info!("Running AssumeUTXO prepare hook");

        // Create datadir and clear contents
        self.base.clear_and_recreate_directory(&args.tmp_data_dir)?;

        // Replace {commit} placeholder in binary path
        let binary_path = args.binary.replace("{commit}", &args.commit);

        // Sync headers
        info!("Syncing headers");
        let datadir_arg = format!("-datadir={}", args.tmp_data_dir.display());
        let connect_arg = format!("-connect={}", args.connect_address);
        let chain_arg = format!("-chain={}", args.network);

        let sync_args = vec![
            datadir_arg.as_str(),
            connect_arg.as_str(),
            "-daemon=0",
            chain_arg.as_str(),
            "-stopatheight=1",
            "-printtoconsole=0",
        ];
        self.execute_bitcoin_command(&binary_path, &sync_args)?;

        // Load snapshot (allow failure with || true)
        info!("Loading snapshot");
        let datadir_arg2 = format!("-datadir={}", args.tmp_data_dir.display());
        let connect_arg2 = format!("-connect={}", args.connect_address);
        let chain_arg2 = format!("-chain={}", args.network);
        let snapshot_arg = format!("-loadutxosnapshot={}", args.snapshot_path.display());

        let snapshot_args = vec![
            datadir_arg2.as_str(),
            connect_arg2.as_str(),
            "-daemon=0",
            chain_arg2.as_str(),
            "-pausebackgroundsync=1",
            snapshot_arg.as_str(),
            "-printtoconsole=0",
        ];
        self.execute_bitcoin_command_allow_failure(&binary_path, &snapshot_args)?;

        Ok(())
    }

    fn conclude(&self, args: &HookArgs) -> Result<()> {
        info!("Running AssumeUTXO conclude hook");

        // Create output directory structure
        let output_path = args
            .out_dir
            .join(&args.commit)
            .join(&args.params_dir)
            .join(args.iteration.to_string());

        info!("Moving debug.log to {}", output_path.display());
        self.base.create_directory(&output_path)?;

        // Determine debug.log source path based on network
        let debug_log_source = if args.network == "main" {
            args.tmp_data_dir.join("debug.log")
        } else {
            args.tmp_data_dir.join(&args.network).join("debug.log")
        };

        let debug_log_dest = output_path.join("debug.log");

        // Move debug.log
        if debug_log_source.exists() {
            fs::rename(&debug_log_source, &debug_log_dest)
                .or_else(|_| -> Result<()> {
                    // If rename fails (e.g., cross-filesystem), fall back to copy and delete
                    fs::copy(&debug_log_source, &debug_log_dest)?;
                    fs::remove_file(&debug_log_source)?;
                    Ok(())
                })
                .with_context(|| {
                    format!(
                        "Failed to move debug.log from {} to {}",
                        debug_log_source.display(),
                        debug_log_dest.display()
                    )
                })?;
        } else {
            debug!("debug.log not found at {}", debug_log_source.display());
        }

        // Clean datadir contents
        self.base.clear_directory(&args.tmp_data_dir)?;

        Ok(())
    }

    fn cleanup(&self, args: &HookArgs) -> Result<()> {
        info!("Running AssumeUTXO cleanup hook");

        // Final cleanup of datadir
        self.base.clear_directory(&args.tmp_data_dir)?;

        Ok(())
    }
}

/// Full IBD hook executor - no snapshot, full initial block download
pub struct FullIbdHookExecutor {
    base: BaseHookExecutor,
}

impl FullIbdHookExecutor {
    pub fn new() -> Self {
        Self {
            base: BaseHookExecutor,
        }
    }
}

impl Default for FullIbdHookExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl HookExecutor for FullIbdHookExecutor {
    fn setup(&self, args: &HookArgs) -> Result<()> {
        info!("Running Full IBD setup hook");
        self.base.create_directory(&args.tmp_data_dir)?;
        self.base.clear_directory(&args.tmp_data_dir)?;

        Ok(())
    }

    fn prepare(&self, args: &HookArgs) -> Result<()> {
        info!("Running Full IBD prepare hook");

        // Create datadir and clear contents
        self.base.clear_and_recreate_directory(&args.tmp_data_dir)?;

        Ok(())
    }

    fn conclude(&self, args: &HookArgs) -> Result<()> {
        info!("Running Full IBD conclude hook");

        // Create output directory structure
        let output_path = args
            .out_dir
            .join(&args.commit)
            .join(&args.params_dir)
            .join(args.iteration.to_string());

        info!("Moving debug.log to {}", output_path.display());
        self.base.create_directory(&output_path)?;

        // Determine debug.log source path based on network
        let debug_log_source = if args.network == "main" {
            args.tmp_data_dir.join("debug.log")
        } else {
            args.tmp_data_dir.join(&args.network).join("debug.log")
        };

        let debug_log_dest = output_path.join("debug.log");

        // Move debug.log
        if debug_log_source.exists() {
            fs::rename(&debug_log_source, &debug_log_dest)
                .or_else(|_| -> Result<()> {
                    // If rename fails (e.g., cross-filesystem), fall back to copy and delete
                    fs::copy(&debug_log_source, &debug_log_dest)?;
                    fs::remove_file(&debug_log_source)?;
                    Ok(())
                })
                .with_context(|| {
                    format!(
                        "Failed to move debug.log from {} to {}",
                        debug_log_source.display(),
                        debug_log_dest.display()
                    )
                })?;
        } else {
            debug!("debug.log not found at {}", debug_log_source.display());
        }

        // Clean datadir contents
        self.base.clear_directory(&args.tmp_data_dir)?;

        Ok(())
    }

    fn cleanup(&self, args: &HookArgs) -> Result<()> {
        info!("Running Full IBD cleanup hook");

        // Final cleanup of datadir
        self.base.clear_directory(&args.tmp_data_dir)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_create_directory() {
        let base_executor = BaseHookExecutor;
        let temp_dir = tempdir().unwrap();
        let test_path = temp_dir.path().join("test").join("nested").join("dir");

        base_executor.create_directory(&test_path).unwrap();
        assert!(test_path.exists());
    }

    #[test]
    fn test_clear_directory() {
        let base_executor = BaseHookExecutor;
        let temp_dir = tempdir().unwrap();
        let test_path = temp_dir.path();

        // Create some test files and directories
        let file_path = test_path.join("test_file.txt");
        fs::write(&file_path, "test content").unwrap();

        let dir_path = test_path.join("test_dir");
        fs::create_dir(&dir_path).unwrap();
        let nested_file = dir_path.join("nested.txt");
        fs::write(&nested_file, "nested content").unwrap();

        // Clear the directory
        base_executor.clear_directory(test_path).unwrap();

        // Verify everything is removed
        assert!(!file_path.exists());
        assert!(!dir_path.exists());

        // Directory itself should still exist
        assert!(test_path.exists());
    }
}
