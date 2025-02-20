use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

const DEFAULT_SCRIPTS: [(&str, &str); 4] = [
    ("setup", "setup.sh"),
    ("prepare", "prepare.sh"),
    ("conclude", "conclude.sh"),
    ("cleanup", "cleanup.sh"),
];

pub struct HookManager {
    scripts_dir: PathBuf,
}

impl HookManager {
    /// Get the current directory where the binary is running
    fn get_base_dir() -> Result<PathBuf> {
        env::current_dir().with_context(|| "Failed to get current directory")
    }

    /// Create a new HookManager using the current directory as base
    pub fn new_from_current() -> Result<Self> {
        Ok(Self::new(&Self::get_base_dir()?))
    }
    pub fn new(base_dir: &Path) -> Self {
        Self {
            scripts_dir: base_dir.join("scripts"),
        }
    }

    /// Add default script hooks to the hyperfine options if they're not already present
    pub fn add_default_hooks(&self, options: &mut HashMap<String, Value>) -> Result<()> {
        // First verify scripts directory exists
        if !self.scripts_dir.exists() {
            anyhow::bail!(
                "Scripts directory not found: {}",
                self.scripts_dir.display()
            );
        }

        // Add each default script if not already present
        for (hook_name, script_path) in DEFAULT_SCRIPTS.iter() {
            if !options.contains_key(*hook_name) {
                let script = self.scripts_dir.join(script_path.trim_start_matches("./"));

                // Verify script exists and is executable
                if !script.exists() {
                    anyhow::bail!("Script not found: {}", script.display());
                }

                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let metadata = script.metadata().with_context(|| {
                        format!("Failed to get metadata for {}", script.display())
                    })?;
                    let permissions = metadata.permissions();
                    if permissions.mode() & 0o111 == 0 {
                        anyhow::bail!("Script is not executable: {}", script.display());
                    }
                }

                options.insert(
                    hook_name.to_string(),
                    Value::String(script.to_string_lossy().into_owned()),
                );
            }
        }

        Ok(())
    }
}
