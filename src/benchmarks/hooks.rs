use anyhow::Result;
use log::debug;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct HookManager {}

impl HookManager {
    /// Create a new HookManager using the current directory as base
    pub fn new() -> Result<Self> {
        Ok(HookManager {})
    }

    /// Add default script hooks to the hyperfine options if they're not already present
    pub fn add_script_hooks(
        &self,
        options: &mut HashMap<String, Value>,
        network: &String,
        tmp_data_dir: PathBuf,
        out_dir: PathBuf,
    ) -> Result<()> {
        let hook_types = ["setup", "conclude", "prepare", "cleanup"];
        // Check if we are using network "mainnet"
        // If we are not, append the "network" to the data_dir_path
        let modified_data_dir = if network == "mainnet" {
            tmp_data_dir
        } else {
            tmp_data_dir.join(network)
        };

        for hook_type in hook_types.iter() {
            if let Some(value) = options.get_mut(*hook_type) {
                if let Some(script) = value.as_str() {
                    // Construct the new script command with directories as arguments
                    let new_script = format!(
                        "{} {} {}",
                        script,
                        modified_data_dir.display(),
                        out_dir.display()
                    );
                    debug!("Adding {hook_type} to options as: {new_script}");

                    // Update the value in the options map
                    *value = Value::String(new_script);
                }
            }
        }

        Ok(())
    }
}
