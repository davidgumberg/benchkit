use anyhow::Result;
use log::debug;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive()]
pub struct ScriptArgs {
    pub binary: String,
    pub connect_address: String,
    pub network: String,
    pub out_dir: PathBuf,
    pub snapshot_path: PathBuf,
    pub tmp_data_dir: PathBuf,
}

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
        script_args: ScriptArgs,
    ) -> Result<()> {
        let hook_types = ["setup", "conclude", "prepare", "cleanup"];
        // // Check if we are using network "mainnet"
        // // If we are not, append the "network" to the data_dir_path
        // let modified_data_dir = if script_args.network == "mainnet" {
        //     script_args.tmp_data_dir
        // } else {
        //     script_args.tmp_data_dir.join(&script_args.network)
        // };

        for hook_type in hook_types.iter() {
            if let Some(value) = options.get_mut(*hook_type) {
                if let Some(script) = value.as_str() {
                    // Construct the new script command with arguments in a fixed order + the
                    // hyperfine iteration counter
                    let new_script = format!(
                        "{} {} {} {} {} {} {} \"$HYPERFINE_ITERATION\" {{commit}}",
                        script,
                        script_args.binary,
                        script_args.connect_address,
                        script_args.network,
                        script_args.out_dir.display(),
                        script_args.snapshot_path.display(),
                        script_args.tmp_data_dir.display(),
                    );
                    debug!("Adding {hook_type} to options as: {new_script}");

                    // Update the value in the options map
                    *value = Value::String(new_script);
                    debug!("Updated command to: {:?}", value.as_str());
                }
            }
        }

        Ok(())
    }
}
