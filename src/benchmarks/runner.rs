use anyhow::{Context, Result};
use clap::ValueEnum;
use log::{debug, info};
use std::path::{Path, PathBuf};

use crate::benchmarks::Builder;
use crate::benchmarks::hook_runner::HookArgs;
use crate::benchmarks::parameters::ParameterList;
use crate::benchmarks::utils::check_binaries_exist;
use crate::config::{get_merged_options, GlobalConfig, SingleConfig};
use crate::download::SnapshotInfo;
use crate::path_utils;
use crate::types::Network;

/// High-level benchmark orchestrator that coordinates benchmark execution
///
/// The Runner is responsible for:
/// 1. Managing configuration and settings
/// 2. Setting up directories and copying configuration files
/// 3. Selecting and iterating through benchmarks to run
/// 4. Creating and configuring BenchmarkRunner instances for actual execution
/// 5. Handling environment setup like snapshots
///
/// It delegates the actual benchmark execution to BenchmarkRunner instances
/// which handle the low-level details of running commands and measuring performance.
pub struct Runner {
    /// Global configuration for the application and benchmarks
    global_config: GlobalConfig,
    /// Directory to store benchmark outputs and results
    out_dir: PathBuf,
}

impl Runner {
    /// Create a new Runner
    pub fn new(global_config: GlobalConfig, out_dir: PathBuf) -> Result<Self> {
        debug!("Using output directory: {}", out_dir.display());

        // Create output directory and check it's empty
        path_utils::prepare_output_directory(&out_dir)?;

        // Copy config files to output directory
        let app_config_name = global_config.app.path.file_name().unwrap_or_default();
        let bench_config_name = global_config.bench.path.file_name().unwrap_or_default();

        path_utils::copy_file(&global_config.app.path, &out_dir.join(app_config_name))?;

        path_utils::copy_file(&global_config.bench.path, &out_dir.join(bench_config_name))?;

        // Dump system info
        crate::system_info::dump_sys_info(&out_dir.join("system_info"))?;

        Ok(Self {
            global_config,
            out_dir,
        })
    }

    /// Run all or a specific benchmark
    pub fn run(&self, name: Option<&str>, build: bool) -> Result<()> {
        if build {
            let mut builder = Builder::new(self.global_config.clone())?;
            builder.build()?;
        }
        // Check if all required binaries exist
        if let Err(missing_binaries) = check_binaries_exist(
            &self.global_config.app.bin_dir,
            &self.global_config.bench.global.commits,
        ) {
            let mut error_msg = String::from("Missing required binaries:\n");
            for (commit, path) in missing_binaries {
                error_msg.push_str(&format!(
                    "  - Binary 'bitcoind-{}' not found at expected path: {}\n",
                    commit,
                    path.display()
                ));
            }
            error_msg.push_str("\nPlease run 'benchkit build' to build the required binaries.");
            anyhow::bail!(error_msg);
        }

        let benchmarks = match name {
            Some(n) => {
                let bench = self
                    .global_config
                    .bench
                    .benchmarks
                    .iter()
                    .enumerate()
                    .find(|(_, b)| b.name == n)
                    .with_context(|| format!("Benchmark not found: {n}"))?;
                vec![bench]
            }
            None => self
                .global_config
                .bench
                .benchmarks
                .iter()
                .enumerate()
                .collect(),
        };

        for (index, bench) in benchmarks {
            self.check_snapshot(bench, &self.global_config.app.snapshot_dir)?;
            self.run_benchmark(index, bench)?;
        }

        Ok(())
    }

    /// Check if required snapshot exists
    fn check_snapshot(&self, bench: &SingleConfig, snapshot_dir: &Path) -> Result<()> {
        // Check if we have the correct snapshot
        let network = Network::from_str(&bench.network, true)
            .map_err(|e| anyhow::anyhow!("{}", e))
            .with_context(|| format!("Invalid network: {:?}", bench.network))?;

        if let Some(snapshot_info) = SnapshotInfo::for_network(&network) {
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

    /// Run a specific benchmark
    fn run_benchmark(&self, index: usize, bench: &SingleConfig) -> Result<()> {
        info!("Running benchmark: {:?}", bench.name);

        // Get merged options for this benchmark
        let options = get_merged_options(&self.global_config.bench, index)?;

        // Create parameter lists for substitution
        let mut parameter_lists = if let Some(params) = &options.parameter_lists {
            crate::benchmarks::parameters::ParameterUtils::create_parameter_lists(
                &serde_json::Value::Array(params.clone()),
            )
            .unwrap_or_default()
        } else {
            Vec::new()
        };

        // Add commits parameter list if not already present
        if !parameter_lists.iter().any(|list| list.var == "commit") {
            parameter_lists.push(ParameterList {
                var: "commit".to_string(),
                values: self.global_config.bench.global.commits.clone(),
            });
        }

        // Create hook runner with appropriate mode
        let mode = if let Some(mode_str) = &bench.mode {
            crate::benchmarks::HookMode::mode_from_str(mode_str)?
        } else {
            crate::benchmarks::HookMode::default()
        };
        let hook_runner = crate::benchmarks::hook_runner::HookRunner::with_mode(mode);

        // Create benchmark runner with optional profiling
        let benchmark_runner = crate::benchmarks::benchmark_runner::BenchmarkRunner::builder(
            self.out_dir.clone(),
            hook_runner,
        )
        .capture_output(options.capture_output)
        .parameter_lists(parameter_lists)
        .profiling(options.profile.unwrap_or(false), options.profile_interval)
        .benchmark_cores(self.global_config.bench.global.benchmark_cores.clone())
        .stop_on_log_pattern(options.stop_on_log_pattern.clone())
        .perf_instrumentation(options.perf_instrumentation.unwrap_or(false))
        .build()?;

        // Get snapshot info
        let snapshot_path = if let Some(snapshot_info) = SnapshotInfo::for_network(
            &Network::from_str(&bench.network, true)
                .map_err(|e| anyhow::anyhow!("{}", e))
                .with_context(|| format!("Invalid network: {:?}", bench.network))?,
        ) {
            self.global_config
                .app
                .snapshot_dir
                .join(snapshot_info.filename)
        } else {
            self.global_config.app.snapshot_dir.clone() // Fallback
        };

        // Get command template
        let command_template = match &options.command {
            Some(cmd) => crate::benchmarks::utils::build_benchmark_command(
                &self.global_config.app.bin_dir,
                "{commit}",
                &bench.network,
                &self.global_config.bench.global.tmp_data_dir,
                &bench.connect.clone().unwrap_or_default(),
                cmd,
            ),
            None => anyhow::bail!(
                "No command template specified for benchmark: {}",
                bench.name
            ),
        };

        // Hooks are the various hyperfine-esque prepare/setup/conclude/cleanup scripts
        let hook_args = HookArgs {
            binary: format!(
                "{}/bitcoind-{{commit}}",
                self.global_config.app.bin_dir.display()
            ),
            connect_address: bench.connect.clone().unwrap_or_default(),
            network: bench.network.clone(),
            out_dir: self.out_dir.clone(),
            snapshot_path,
            tmp_data_dir: self.global_config.bench.global.tmp_data_dir.clone(),
            iteration: 0,
            commit: "{commit}".to_string(), // Will be replaced by parameter substitution
            params_dir: "default".to_string(), // Will be updated during parameter matrix expansion
        };

        let results =
            benchmark_runner.run_parameter_matrix(&command_template, options.runs, &hook_args)?;

        let export_path = self.out_dir.join("results.json");
        crate::benchmarks::benchmark_runner::BenchmarkRunner::export_json_multiple(
            &results,
            &export_path,
        )?;

        info!("Benchmark {} completed successfully", bench.name);
        Ok(())
    }
}
