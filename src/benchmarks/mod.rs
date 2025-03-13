use anyhow::{Context, Result};
use clap::ValueEnum;
use log::{debug, info};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

mod build;
pub use build::Builder;
mod config;
pub use config::{load_bench_config, BenchmarkConfig, BenchmarkGlobalConfig, SingleConfig};
mod hooks;
use hooks::{HookManager, ScriptArgs};
// mod object_storage;
// pub use object_storage::ObjectStorage;

use crate::config::GlobalConfig;
use crate::system_info::dump_sys_info;
use crate::types::Network;

pub struct Runner {
    config: GlobalConfig,
    out_dir: PathBuf,
}

impl Runner {
    pub fn new(
        config: GlobalConfig,
        out_dir: PathBuf,
        app_config: &PathBuf,
        bench_config: &PathBuf,
    ) -> Result<Self> {
        // Configure stage
        debug!("Using output directory: {}", out_dir.display());
        std::fs::create_dir_all(&out_dir)?;
        if std::fs::read_dir(&out_dir)?.next().is_some() {
            anyhow::bail!(
                "Output directory '{}' is not empty. Please clear it before running benchmarks",
                out_dir.display()
            );
        }
        let app_config_name = app_config.file_name().unwrap_or_default();
        let bench_config_name = bench_config.file_name().unwrap_or_default();
        std::fs::copy(app_config, out_dir.join(app_config_name))?;
        std::fs::copy(bench_config, out_dir.join(bench_config_name))?;
        dump_sys_info(&out_dir.join("system_info"))?;
        Ok(Self { config, out_dir })
    }

    pub async fn run(&self, name: Option<&str>) -> Result<()> {
        let benchmarks = match name {
            Some(n) => {
                let bench = self
                    .config
                    .bench
                    .benchmarks
                    .iter()
                    .find(|b| b.name == n)
                    .with_context(|| format!("Benchmark not found: {}", n))?;
                vec![bench]
            }
            None => self.config.bench.benchmarks.iter().collect(),
        };

        for bench in benchmarks {
            // TODO: Remove this check to enable runs without AssumeUTXO
            self.check_snapshot(bench, &self.config.app.snapshot_dir)
                .await?;
            self.run_benchmark(bench).await?;
        }
        Ok(())
    }

    async fn check_snapshot(&self, bench: &SingleConfig, snapshot_dir: &Path) -> Result<()> {
        // Check if we have the correct snapshot
        let network = Network::from_str(&bench.network, true)
            .map_err(|e| anyhow::anyhow!("{}", e))
            .with_context(|| format!("Invalid network: {:?}", bench.network))?;

        if let Some(snapshot_info) = crate::download::SnapshotInfo::for_network(&network) {
            let snapshot_path = snapshot_dir.join(snapshot_info.filename);
            if !snapshot_path.exists() {
                anyhow::bail!(
                    "Missing required snapshot file for network {}: {}\n
This can be downloaded with `benchkit snapshot download {}`",
                    bench.network,
                    snapshot_path.display(),
                    bench.network
                );
            }
        }

        Ok(())
    }

    async fn run_benchmark(&self, bench: &SingleConfig) -> Result<()> {
        info!("Running benchmark: {:?}", bench);

        // Merge hyperfine options from global and benchmark configs
        let mut merged_hyperfine = HashMap::new();
        if let Some(global_opts) = &self.config.bench.global.hyperfine {
            merged_hyperfine.extend(global_opts.clone());
        }
        merged_hyperfine.extend(bench.hyperfine.clone());

        // Update command to use full binary path and apply chain= param
        if let Some(Value::String(command)) = merged_hyperfine.get_mut("command") {
            let new_command = command.replace(
                "bitcoind",
                &format!(
                    "{}/bitcoind-{{commit}} -chain={} -datadir={} -connect={}",
                    self.config.app.bin_dir.display(),
                    bench.network,
                    self.config.bench.global.tmp_data_dir.display(),
                    // TODO: handle if this is empty
                    bench.connect.clone().unwrap().as_str(),
                ),
            );
            *command = new_command;
        }

        // Get the snapshot info and construct full path
        let snapshot_path = if let Some(snapshot_info) = crate::download::SnapshotInfo::for_network(
            &Network::from_str(&bench.network, true)
                .map_err(|e| anyhow::anyhow!("{}", e))
                .with_context(|| format!("Invalid network: {:?}", bench.network))?,
        ) {
            self.config.app.snapshot_dir.join(snapshot_info.filename)
        } else {
            self.config.app.snapshot_dir.clone() // Fallback to directory if no snapshot info
        };

        // Add script hooks
        let script_args = ScriptArgs {
            binary: format!("{}/bitcoind-{{commit}}", self.config.app.bin_dir.display()),
            connect_address: bench.connect.clone().unwrap_or_default(),
            out_dir: self.out_dir.clone(),
            network: bench.network.clone(),
            snapshot_path,
            tmp_data_dir: self.config.bench.global.tmp_data_dir.clone(),
        };
        let hook_manager =
            HookManager::new().with_context(|| "Failed to initialize hook manager")?;
        hook_manager
            .add_script_hooks(&mut merged_hyperfine, script_args)
            .with_context(|| "Failed to add hyperfine script hooks")?;

        // Add commits to parameter-lists if not already present
        let parameter_lists = merged_hyperfine
            .entry("parameter_lists".to_string())
            .or_insert_with(|| Value::Array(Vec::new()));

        if let Value::Array(lists) = parameter_lists {
            // Check if commits parameter list already exists
            if !lists
                .iter()
                .any(|list| (list.get("var").and_then(Value::as_str) == Some("commit")))
            {
                // Add commits parameter list
                lists.push(json!({
                    "var": "commit",
                    // "values": full_commits
                    "values": self.config.bench.global.commits
                }));
            }
        }

        // Hardcode the export path to the top-level of the out_dir
        let export_path = self.out_dir.join("results.json");
        merged_hyperfine.insert(
            "export_json".to_string(),
            Value::String(export_path.to_string_lossy().into_owned()),
        );

        // Run hyperfine with merged options
        self.run_hyperfine(bench, &merged_hyperfine)?;

        // Check for and process results
        if !Path::new(&export_path).exists() {
            anyhow::bail!(
                "Expected JSON results file not found at '{}' for benchmark '{}'",
                export_path.display(),
                bench.name
            );
        }
        Ok(())
    }

    fn run_hyperfine(
        &self,
        bench: &SingleConfig,
        merged_opts: &HashMap<String, Value>,
    ) -> Result<()> {
        let mut cmd = self.build_hyperfine_command(bench, merged_opts)?;

        debug!("Running hyperfine command: {:?}", cmd);
        let status = cmd.status().with_context(|| {
            format!("Failed to execute hyperfine for benchmark '{}'", bench.name)
        })?;

        if !status.success() {
            anyhow::bail!("hyperfine failed for benchmark '{}'", bench.name);
        }

        Ok(())
    }

    fn build_hyperfine_command(
        &self,
        bench: &SingleConfig,
        options: &HashMap<String, Value>,
    ) -> Result<Command> {
        let mut cmd = Command::new("hyperfine");
        let command_str;

        // // Add hook script paths to command
        // for (name, path) in script_manager.get_script_paths() {
        //     cmd.arg(format!("--{}", name)).arg(path);
        // }

        if let Some(Value::String(command)) = options.get("command") {
            command_str = if let Some(wrapper) = &self.config.bench.global.wrapper {
                format!("{} {}", wrapper, command)
            } else {
                command.clone()
            };
        } else {
            anyhow::bail!("command is required in hyperfine config");
        }

        for (key, value) in options {
            if key == "command" {
                continue; // Already handled
            }
            let arg_key = format!("--{}", key.replace('_', "-"));
            match value {
                Value::String(s) => {
                    cmd.arg(arg_key).arg(s);
                }
                Value::Number(n) => {
                    cmd.arg(arg_key).arg(n.to_string());
                }
                Value::Array(arr) => {
                    if key == "command_names" {
                        for name in arr {
                            if let Some(name_str) = name.as_str() {
                                cmd.arg("--command-name").arg(name_str);
                            }
                        }
                    } else if key == "parameter_lists" {
                        for list in arr {
                            match list {
                                Value::Object(map) => {
                                    let var = map.get("var").and_then(Value::as_str).with_context(
                                        || "Missing or invalid 'var' in parameter_lists",
                                    )?;

                                    let values =
                                        map.get("values").and_then(Value::as_array).with_context(
                                            || "Missing or invalid 'values' in parameter_lists",
                                        )?;

                                    let values_str: Vec<String> = values
                                        .iter()
                                        .map(|v| {
                                            v.as_str().map(String::from).with_context(|| {
                                                format!("Invalid value in parameter_lists: {:?}", v)
                                            })
                                        })
                                        .collect::<Result<_>>()?;

                                    cmd.arg("--parameter-list")
                                        .arg(var)
                                        .arg(values_str.join(","));
                                }
                                _ => anyhow::bail!("Invalid parameter_lists entry: {:?}", list),
                            }
                        }
                    }
                }
                Value::Bool(b) => {
                    if *b {
                        cmd.arg(arg_key);
                    }
                }
                _ => {}
            }
        }

        cmd.arg(command_str);

        if let Some(env_map) = &bench.env {
            for (k, v) in env_map {
                cmd.env(k, v);
            }
        }

        debug!("Built hyperfine command: {:?}", cmd);
        Ok(cmd)
    }
}
