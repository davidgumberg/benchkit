use anyhow::{Context, Result};
use log::debug;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::command::{CommandContext, CommandExecutor};

/// Represents the different hook script stages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// Builder for HookRunner
pub struct HookRunnerBuilder {
    /// Directory containing hook scripts
    script_dir: PathBuf,
    /// Custom script paths for specific stages (overrides defaults)
    custom_scripts: HashMap<HookStage, PathBuf>,
}

impl HookRunnerBuilder {
    /// Create a new HookRunnerBuilder with the script directory
    pub fn new(script_dir: PathBuf) -> Self {
        Self {
            script_dir,
            custom_scripts: HashMap::new(),
        }
    }

    /// Add a custom script for a specific stage
    pub fn custom_script(mut self, stage: HookStage, script_path: PathBuf) -> Self {
        self.custom_scripts.insert(stage, script_path);
        self
    }

    /// Build the HookRunner instance
    pub fn build(self) -> Result<HookRunner> {
        // Validate that the script directory exists
        if !self.script_dir.exists() {
            return Err(anyhow::anyhow!(
                "Script directory does not exist: {}",
                self.script_dir.display()
            ));
        }

        Ok(HookRunner {
            script_dir: self.script_dir,
            custom_scripts: self.custom_scripts,
        })
    }
}

/// HookRunner manages the lifecycle scripts for benchmarks
pub struct HookRunner {
    /// Directory containing hook scripts
    script_dir: PathBuf,
    /// Custom script paths for specific stages (overrides defaults)
    custom_scripts: HashMap<HookStage, PathBuf>,
}

impl HookRunner {
    /// Create a new HookRunner
    pub fn new(script_dir: PathBuf) -> Self {
        Self {
            script_dir,
            custom_scripts: HashMap::new(),
        }
    }

    /// Create a new HookRunnerBuilder
    pub fn builder(script_dir: PathBuf) -> HookRunnerBuilder {
        HookRunnerBuilder::new(script_dir)
    }

    /// Add benchmark-specific script
    pub fn with_custom_script(mut self, stage: HookStage, script_path: PathBuf) -> Self {
        self.custom_scripts.insert(stage, script_path);
        self
    }

    /// Run a hook script for the stage
    pub fn run_hook(&self, stage: HookStage, args: &HookArgs) -> Result<()> {
        let script_path = if let Some(custom_path) = self.custom_scripts.get(&stage) {
            custom_path.clone()
        } else {
            self.script_dir.join(stage.script_name())
        };

        if !script_path.exists() {
            debug!("Script {} does not exist, skipping", script_path.display());
            return Ok(());
        }

        debug!(
            "Running {} script: {}",
            format!("{:?}", stage).to_lowercase(),
            script_path.display()
        );

        // Replace {commit} placeholder in binary path with actual commit
        let binary_path = args.binary.replace("{commit}", &args.commit);

        let script_args = vec![
            format!("--binary={}", binary_path),
            format!("--connect={}", args.connect_address),
            format!("--network={}", args.network),
            format!("--out-dir={}", args.out_dir.display()),
            format!("--snapshot={}", args.snapshot_path.display()),
            format!("--datadir={}", args.tmp_data_dir.display()),
            format!("--iteration={}", args.iteration),
            format!("--commit={}", args.commit),
            format!("--params-dir={}", args.params_dir),
        ];
        let script_args_refs: Vec<&str> = script_args.iter().map(|s| s.as_str()).collect();
        let context = CommandContext {
            command_name: Some(format!("{:?} script", stage)),
            allow_failure: false,
            ..CommandContext::default()
        };
        let executor = CommandExecutor::with_context(context);
        let status = executor
            .execute_check_status(script_path.to_str().unwrap_or_default(), &script_args_refs)
            .with_context(|| {
                format!(
                    "Failed to execute {} script",
                    format!("{:?}", stage).to_lowercase()
                )
            })?;
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
