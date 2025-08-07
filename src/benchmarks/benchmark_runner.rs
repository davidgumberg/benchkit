use anyhow::{Context, Result};
use log::{debug, info, warn};
use std::collections::HashMap;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::time::Instant;

use crate::benchmarks::export::ResultExporter;
use crate::benchmarks::hook_runner::{HookArgs, HookRunner, HookStage};
use crate::benchmarks::log_monitor::LogMonitor;
use crate::benchmarks::parameters::{ParameterList, ParameterMatrix, ParameterUtils};
use crate::benchmarks::profiler::{ProfileResult, Profiler};
use crate::benchmarks::results::{BenchmarkResult, ResultAnalyzer, RunResult};
use crate::command::CommandExecutor;

/// Low-level benchmark executor that handles the actual command execution and measurement
/// It is created and configured by the Runner for each benchmark, and focuses
/// solely on the execution details without knowledge of the broader configuration
/// or benchmark selection.
pub struct BenchmarkRunner {
    /// Hook runner for lifecycle scripts
    hook_runner: HookRunner,
    /// Whether to capture command output
    capture_output: bool,
    /// Parameter matrix for running template commands
    parameter_matrix: Option<ParameterMatrix>,
    /// Whether to enable profiling
    enable_profiling: bool,
    /// Directory to store profiling output
    out_dir: PathBuf,
    /// Sampling interval for profiling in seconds
    profile_interval: u64,
    /// Cores to constrain benchmarks to
    benchmark_cores: Option<String>,
    /// Optional regex pattern to stop the benchmark when matched
    stop_on_log_pattern: Option<String>,
}

/// Builder for BenchmarkRunner
pub struct BenchmarkRunnerBuilder {
    hook_runner: HookRunner,
    capture_output: bool,
    parameter_matrix: Option<ParameterMatrix>,
    enable_profiling: bool,
    out_dir: PathBuf,
    profile_interval: u64,
    benchmark_cores: Option<String>,
    stop_on_log_pattern: Option<String>,
}

impl BenchmarkRunnerBuilder {
    /// Create a new BenchmarkRunnerBuilder with required parameters
    pub fn new(out_dir: PathBuf, hook_runner: HookRunner) -> Self {
        Self {
            hook_runner,
            capture_output: false,
            parameter_matrix: None,
            enable_profiling: false,
            out_dir,
            profile_interval: 5, // Default to 5 second interval
            benchmark_cores: None,
            stop_on_log_pattern: None,
        }
    }

    /// Set whether to capture command output
    pub fn capture_output(mut self, capture: bool) -> Self {
        self.capture_output = capture;
        self
    }

    /// Set benchmark cores to constrain command execution
    pub fn benchmark_cores(mut self, cores_spec: Option<String>) -> Self {
        self.benchmark_cores = cores_spec;
        self
    }

    /// Enable profiling with the specified sampling interval
    pub fn profiling(mut self, enable: bool, interval: Option<u64>) -> Self {
        self.enable_profiling = enable;
        if let Some(interval) = interval {
            self.profile_interval = interval;
        }
        self
    }

    /// Set parameter lists for this benchmark runner
    pub fn parameter_lists(mut self, parameter_lists: Vec<ParameterList>) -> Self {
        self.parameter_matrix = Some(ParameterMatrix::new(&parameter_lists));
        self
    }

    /// Set stop on log pattern (regex)
    pub fn stop_on_log_pattern(mut self, pattern: Option<String>) -> Self {
        self.stop_on_log_pattern = pattern;
        self
    }

    /// Build the BenchmarkRunner, validating parameters if needed
    pub fn build(self) -> Result<BenchmarkRunner> {
        // Validate configuration if needed
        // For example, ensure out_dir exists or can be created

        // Create the BenchmarkRunner
        Ok(BenchmarkRunner {
            hook_runner: self.hook_runner,
            capture_output: self.capture_output,
            parameter_matrix: self.parameter_matrix,
            enable_profiling: self.enable_profiling,
            out_dir: self.out_dir,
            profile_interval: self.profile_interval,
            benchmark_cores: self.benchmark_cores,
            stop_on_log_pattern: self.stop_on_log_pattern,
        })
    }
}

impl BenchmarkRunner {
    /// Create a builder for BenchmarkRunner
    pub fn builder(out_dir: PathBuf, hook_runner: HookRunner) -> BenchmarkRunnerBuilder {
        BenchmarkRunnerBuilder::new(out_dir, hook_runner)
    }

    /// Run a benchmark command with the specified number of runs
    pub fn run_benchmark(
        &self,
        command: &str,
        runs: usize,
        hook_args: &HookArgs,
    ) -> Result<BenchmarkResult> {
        // Use the variant with empty parameters
        let empty_params = HashMap::new();
        self.run_benchmark_with_params(command, runs, hook_args, &empty_params)
    }

    /// Run a benchmark command with the specified number of runs and parameter values
    pub fn run_benchmark_with_params(
        &self,
        command: &str,
        runs: usize,
        hook_args: &HookArgs,
        params: &HashMap<String, String>,
    ) -> Result<BenchmarkResult> {
        let commit = &hook_args.commit;
        info!(
            "Running benchmark: {command} for {runs} runs (commit: {commit})"
        );

        // Run the setup script once before all benchmark runs
        self.hook_runner.run_hook(HookStage::Setup, hook_args)?;
        let mut results = Vec::with_capacity(runs);

        for i in 0..runs {
            // Create iteration-specific hook args with parameter directory
            let params_dir = ParameterUtils::params_to_dirname(params);
            let iter_args = HookArgs {
                iteration: i,
                params_dir: params_dir.clone(),
                ..hook_args.clone()
            };

            // Run prepare script before the benchmark run
            self.hook_runner.run_hook(HookStage::Prepare, &iter_args)?;
            let start = Instant::now();
            let (output, profile_result) = self.execute_command(command, i, commit, params)?;
            // Stop timing (if we're not profiling, otherwise the profiler takes care of timing)
            let duration = start.elapsed();
            let duration_ms = if let Some(profile) = &profile_result {
                // Use the duration from the profiler if available
                profile.duration * 1000.0
            } else {
                duration.as_secs_f64() * 1000.0
            };

            // Record result
            let run_result = RunResult {
                iteration: i,
                duration_ms,
                exit_code: output.status.code().unwrap_or(-1),
                output: if self.capture_output {
                    // Only store output if explicitly requested
                    Some(String::from_utf8_lossy(&output.stdout).to_string())
                } else {
                    None
                },
                profile: profile_result,
            };

            results.push(run_result);

            // Run conclude script after the benchmark run
            self.hook_runner.run_hook(HookStage::Conclude, &iter_args)?;
        }

        // Run the cleanup script once after all benchmark runs
        self.hook_runner.run_hook(HookStage::Cleanup, hook_args)?;

        // Calculate statistics
        let summary = ResultAnalyzer::calculate_summary(&results);

        // Create the benchmark result
        let benchmark_result = BenchmarkResult {
            command: command.to_string(),
            parameters: params.clone(), // Copy the parameters into the result
            runs: results,
            summary,
        };

        Ok(benchmark_result)
    }

    /// Launch a command with CPU affinity constraints
    /// This is a helper function that can be used by both regular execution and profiling
    fn launch_command_with_affinity(&self, command: &str) -> Result<std::process::Child> {
        debug!("Launching command with affinity: {command}");

        // Determine if we need to capture output
        // We capture output if:
        // 1. capture_output is true (for storing in results)
        // 2. stop_on_log_pattern is configured (for monitoring)
        // 3. We're not profiling (profiling doesn't capture output)
        let should_capture =
            !self.enable_profiling && (self.capture_output || self.stop_on_log_pattern.is_some());

        // Create a command executor with our benchmark settings
        let executor = CommandExecutor::builder()
            .name(command.to_string())
            .cpu_cores(self.benchmark_cores.clone())
            .process_group(true)
            .capture_output(should_capture)
            .build()?;

        // Launch the command using the executor
        executor.launch_command("sh", &["-c", command])
    }

    /// Execute a command and capture its output, optionally with profiling
    fn execute_command(
        &self,
        command: &str,
        iteration: usize,
        commit: &str,
        params: &HashMap<String, String>,
    ) -> Result<(std::process::Output, Option<ProfileResult>)> {
        // Automatically append -printtoconsole if stop_on_log_pattern is configured
        // and the command doesn't already contain it
        let final_command =
            if self.stop_on_log_pattern.is_some() && !command.contains("-printtoconsole") {
                let updated_command = format!("{command} -printtoconsole");
                debug!(
                    "Automatically added -printtoconsole for log pattern matching: {updated_command}"
                );
                updated_command
            } else {
                command.to_string()
            };

        debug!("Executing command: {final_command}");

        // Check for conflicts between profiling and stop_on_log_pattern
        if self.enable_profiling && self.stop_on_log_pattern.is_some() {
            warn!("Both profiling and stop_on_log_pattern are configured. Profiling takes precedence, stop_on_log_pattern will be ignored for this run.");
        }

        // If profiling is enabled, use the profiler to execute the command
        if self.enable_profiling {
            // Create a directory structure with commit/params/iteration
            let params_dir = ParameterUtils::params_to_dirname(params);
            let profile_out_dir = self
                .out_dir
                .join(commit)
                .join(params_dir)
                .join(iteration.to_string());
            std::fs::create_dir_all(&profile_out_dir)?;

            // Create the profiler with our benchmark cores
            let mut profiler = Profiler::builder(&profile_out_dir)
                .sample_interval(self.profile_interval)
                .benchmark_cores(self.benchmark_cores.clone())
                .build()?;

            // Launch the command using our helper, which handles CPU affinity
            info!("Profiling command: {final_command}");
            let child = self.launch_command_with_affinity(&final_command)?;
            let profile_result = profiler.profile_process(&final_command, child)?;

            // Make an Output manually for profile
            let output = std::process::Output {
                status: ExitStatusExt::from_raw(profile_result.exit_code),
                stdout: Vec::new(),
                stderr: Vec::new(),
            };

            return Ok((output, Some(profile_result)));
        }

        // For non-profiled commands, launch and potentially monitor
        let mut child = self.launch_command_with_affinity(&final_command)?;

        // If stop_on_log_pattern is configured, monitor the output
        if let Some(pattern) = &self.stop_on_log_pattern {
            info!("Monitoring command output for pattern: {pattern}");

            // Start monitoring the child process
            let mut monitor = LogMonitor::start_monitoring(&mut child, pattern.clone())?;

            // Wait for either pattern match or process exit
            let pattern_matched = monitor
                .wait_for_match_or_exit(&mut child, std::time::Duration::from_millis(100))?;

            if pattern_matched {
                // Pattern was matched, terminate the process gracefully
                info!("Pattern matched, terminating process");

                // Send SIGTERM to the process
                match child.kill() {
                    Ok(_) => debug!("Process terminated successfully"),
                    Err(e) => warn!("Failed to terminate process: {e}"),
                }

                // Also try to terminate any child processes via process group
                if let Some(pgid) = child.id().checked_neg() {
                    unsafe {
                        libc::kill(pgid as i32, libc::SIGTERM);
                    }
                }
            }
        }

        // Wait for the command to complete and collect output
        let output = child
            .wait_with_output()
            .context("Failed to wait for command completion")?;

        if !output.status.success() {
            debug!(
                "Command failed with status: {}",
                output.status.code().unwrap_or(-1)
            );
            // We don't return an error here because we want to capture benchmark failures
            // and include them in the results
        }

        Ok((output, None))
    }

    /// Run all parameter combinations for a command template
    pub fn run_parameter_matrix(
        &self,
        command_template: &str,
        runs: usize,
        hook_args: &HookArgs,
    ) -> Result<Vec<BenchmarkResult>> {
        // If no parameter matrix is set, just run the command as-is
        if self.parameter_matrix.is_none() {
            let result = self.run_benchmark(command_template, runs, hook_args)?;
            return Ok(vec![result]);
        }

        let matrix = self.parameter_matrix.as_ref().unwrap();
        let commands = matrix.generate_commands(command_template);
        let mut results = Vec::with_capacity(commands.len());

        for (command, params) in commands {
            info!("Running command with parameters: {params:?}");

            // Create a new hook_args with the specific commit for this parameter combination
            let mut current_hook_args = hook_args.clone();

            // Update the commit if it's in the params
            if let Some(commit) = params.get("commit") {
                current_hook_args.commit = commit.clone();
            }

            // Create a modified copy of run_benchmark that uses the params for directory structure
            let mut result =
                self.run_benchmark_with_params(&command, runs, &current_hook_args, &params)?;
            result.parameters = params.clone();
            results.push(result);
        }

        Ok(results)
    }

    // Export methods - delegating to the ResultExporter

    pub fn export_json(result: &BenchmarkResult, path: &impl AsRef<std::path::Path>) -> Result<()> {
        ResultExporter::export_json(result, path.as_ref())
    }

    pub fn export_json_multiple(
        results: &[BenchmarkResult],
        path: &impl AsRef<std::path::Path>,
    ) -> Result<()> {
        ResultExporter::export_json_multiple(results, path.as_ref())
    }
}
