use anyhow::{Context, Result};
use log::debug;
use std::path::PathBuf;
use std::process::Command;

/// Represents the different hook script stages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookStage {
    Setup,
    Prepare,
    Conclude,
    Cleanup,
}

impl HookStage {
    /// Get the filename for this hook stage
    pub fn script_name(&self) -> &'static str {
        match self {
            HookStage::Setup => "setup.sh",
            HookStage::Prepare => "prepare.sh",
            HookStage::Conclude => "conclude.sh",
            HookStage::Cleanup => "cleanup.sh",
        }
    }
}

/// Arguments to pass to hook scripts
#[derive(Debug, Clone)]
pub struct HookArgs {
    /// Path to the binary being benchmarked
    pub binary: String,
    /// Address to connect to (e.g., for Bitcoin Core)
    pub connect_address: String,
    /// Network to use (e.g., mainnet, testnet, signet)
    pub network: String,
    /// Output directory for benchmark results
    pub out_dir: PathBuf,
    /// Path to snapshot file
    pub snapshot_path: PathBuf,
    /// Temporary data directory for the benchmarked process
    pub tmp_data_dir: PathBuf,
    /// Current iteration number
    pub iteration: usize,
    /// Commit being benchmarked
    pub commit: String,
    /// Parameter string for directory organization (always present, "default" if no params)
    pub params_dir: String,
}

/// HookRunner manages the lifecycle scripts for benchmarks
pub struct HookRunner {
    /// Directory containing hook scripts
    script_dir: PathBuf,
}

impl HookRunner {
    /// Create a new HookRunner
    pub fn new(script_dir: PathBuf) -> Self {
        Self { script_dir }
    }

    /// Run a hook script for the given stage with the provided arguments
    pub fn run_hook(&self, stage: HookStage, args: &HookArgs) -> Result<()> {
        let script_path = self.script_dir.join(stage.script_name());

        // Check if the script exists
        if !script_path.exists() {
            debug!("Script {} does not exist, skipping", script_path.display());
            return Ok(());
        }

        debug!(
            "Running {} script: {}",
            format!("{:?}", stage).to_lowercase(),
            script_path.display()
        );

        // Build command with named arguments
        let mut cmd = Command::new(&script_path);

        // Replace {commit} placeholder in binary path with actual commit
        let binary_path = args.binary.replace("{commit}", &args.commit);

        cmd.arg(format!("--binary={}", binary_path))
            .arg(format!("--connect={}", args.connect_address))
            .arg(format!("--network={}", args.network))
            .arg(format!("--out-dir={}", args.out_dir.display()))
            .arg(format!("--snapshot={}", args.snapshot_path.display()))
            .arg(format!("--datadir={}", args.tmp_data_dir.display()))
            .arg(format!("--iteration={}", args.iteration))
            .arg(format!("--commit={}", args.commit));

        // Always add params directory
        cmd.arg(format!("--params-dir={}", args.params_dir));

        // Run the command
        let status = cmd.status().context(format!(
            "Failed to execute {} script",
            format!("{:?}", stage).to_lowercase()
        ))?;

        if !status.success() {
            return Err(anyhow::anyhow!(
                "{} script failed with status {}",
                format!("{:?}", stage).to_lowercase(),
                status.code().unwrap_or(-1)
            ));
        }

        Ok(())
    }
}
