use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

mod build;
pub use build::Builder;
mod config;
pub use config::{load_bench_config, BenchmarkConfig, GlobalConfig, SingleConfig};

use crate::config::AppConfig;

pub struct Runner {
    app_config: AppConfig,
    bench_config: BenchmarkConfig,
    database_url: String,
    pull_request_number: Option<i32>,
    run_id: Option<i32>,
}

impl Runner {
    pub fn new(
        app_config: &AppConfig,
        bench_config: &BenchmarkConfig,
        database_url: &str,
        pull_request_number: Option<i32>,
        run_id: Option<i32>,
    ) -> Result<Self> {
        Ok(Self {
            app_config: app_config.clone(),
            bench_config: bench_config.clone(),
            database_url: database_url.to_string(),
            pull_request_number,
            run_id,
        })
    }

    pub async fn run(&self) -> Result<()> {
        for bench in &self.bench_config.benchmarks {
            self.run_benchmark(bench).await?;
        }
        Ok(())
    }

    pub async fn run_single(&self, name: &str) -> Result<()> {
        let bench = self
            .bench_config
            .benchmarks
            .iter()
            .find(|b| b.name == name)
            .with_context(|| format!("Benchmark not found: {}", name))?;

        self.run_benchmark(bench).await
    }

    async fn run_benchmark(&self, bench: &SingleConfig) -> Result<()> {
        println!("Running benchmark: {:?}", bench);

        // First read the global.commits field and expand commit hashes
        let mut full_commits = Vec::new();
        for commit in &self.bench_config.global.commits {
            let output = Command::new("git")
                .current_dir(&self.bench_config.global.source)
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

            println!("Resolved commit {} to full hash {}", commit, full_hash);
            full_commits.push(full_hash);
        }

        // Merge hyperfine options from global and benchmark configs
        let mut merged_hyperfine = HashMap::new();
        if let Some(global_opts) = &self.bench_config.global.hyperfine {
            merged_hyperfine.extend(global_opts.clone());
        }
        merged_hyperfine.extend(bench.hyperfine.clone());

        // Add commits to parameter-lists if not already present
        let param_lists = merged_hyperfine
            .entry("parameter_lists".to_string())
            .or_insert_with(|| Value::Array(Vec::new()));

        if let Value::Array(lists) = param_lists {
            // Check if commits parameter list already exists
            if !lists
                .iter()
                .any(|list| (list.get("var").and_then(Value::as_str) == Some("commit")))
            {
                // Add commits parameter list
                lists.push(json!({
                    "var": "commit",
                    "values": full_commits
                }));
            }
        }

        // Update command to use full binary path
        if let Some(Value::String(command)) = merged_hyperfine.get_mut("command") {
            let new_command = command.replace(
                "bitcoind",
                &format!("{}/bitcoind-{{commit}}", self.app_config.bin_dir.display()),
            );
            *command = new_command;
        }

        // Get the export path before running hyperfine
        let export_path = merged_hyperfine
            .get("export_json")
            .and_then(Value::as_str)
            .with_context(|| {
                format!(
                    "Missing required 'export_json' field in benchmark '{}'",
                    bench.name
                )
            })?;

        // Run hyperfine with merged options
        self.run_hyperfine(bench, &merged_hyperfine)?;

        // Check for and process results
        if !Path::new(export_path).exists() {
            anyhow::bail!(
                "Expected JSON results file not found at '{}' for benchmark '{}'",
                export_path,
                bench.name
            );
        }

        let results_json = std::fs::read_to_string(export_path)
            .with_context(|| format!("Failed to read results file: {}", export_path))?;

        // Store results in database
        crate::database::store_results(
            &self.database_url,
            &bench.name,
            &results_json,
            self.pull_request_number,
            self.run_id,
        )
        .await?;

        // Cleanup
        std::fs::remove_file(export_path)
            .with_context(|| format!("Failed to remove results file: {}", export_path))?;

        Ok(())
    }

    fn run_hyperfine(
        &self,
        bench: &SingleConfig,
        merged_opts: &HashMap<String, Value>,
    ) -> Result<()> {
        let mut cmd = self.build_hyperfine_command(bench, merged_opts)?;

        println!("Running hyperfine command: {:?}", cmd);
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

        if let Some(Value::String(command)) = options.get("command") {
            command_str = if let Some(wrapper) = &self.bench_config.global.wrapper {
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

        println!("Built hyperfine command: {:?}", cmd);
        Ok(cmd)
    }
}
