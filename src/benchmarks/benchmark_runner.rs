use anyhow::{Context, Result};
use log::{debug, info, warn};
use std::collections::HashMap;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::time::Instant;

use crate::benchmarks::export::ResultExporter;
use crate::benchmarks::flamegraph::Flamegrapher;
use crate::benchmarks::hook_runner::{HookArgs, HookRunner, HookStage};
use crate::benchmarks::log_monitor::LogMonitor;
use crate::benchmarks::parameters::{ParameterList, ParameterMatrix, ParameterUtils};
use crate::benchmarks::perf::PerfInstrumentor;
use crate::benchmarks::profiler::{ProfileResult, Profiler};
use crate::benchmarks::results::{BenchmarkResult, InstrumentationType, ResultAnalyzer, RunResult};
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
    /// Directory to store profiling output
    out_dir: PathBuf,
    /// Sampling interval for profiling in seconds
    profile_interval: u64,
    /// Cores to constrain benchmarks to
    benchmark_cores: Option<String>,
    /// Optional regex pattern to stop the benchmark when matched
    stop_on_log_pattern: Option<String>,
    /// Whether to enable profiling
    instrumentation_mode: InstrumentationType,
}

/// Builder for BenchmarkRunner
pub struct BenchmarkRunnerBuilder {
    hook_runner: HookRunner,
    capture_output: bool,
    parameter_matrix: Option<ParameterMatrix>,
    out_dir: PathBuf,
    profile_interval: u64,
    benchmark_cores: Option<String>,
    stop_on_log_pattern: Option<String>,
    instrumentation_mode: InstrumentationType,
}

impl BenchmarkRunnerBuilder {
    pub fn new(out_dir: PathBuf, hook_runner: HookRunner) -> Self {
        Self {
            hook_runner,
            capture_output: false,
            parameter_matrix: None,
            out_dir,
            profile_interval: 5, // Default to 5 second interval
            benchmark_cores: None,
            stop_on_log_pattern: None,
            instrumentation_mode: InstrumentationType::None,
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
        if enable {
            self.instrumentation_mode = InstrumentationType::Profiling;

            if let Some(interval) = interval {
                self.profile_interval = interval;
            }
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

    /// Enable perf instrumentation
    pub fn perf_instrumentation(mut self, enable: bool) -> Self {
        if enable {
            self.instrumentation_mode = InstrumentationType::Perf;
        }
        self
    }

    pub fn flamegraph_instrumentation(mut self, enable: bool) -> Self {
        if enable {
            self.instrumentation_mode = InstrumentationType::Flamegraph;
        }
        self
    }

    /// Build the BenchmarkRunner, validating parameters if needed
    pub fn build(self) -> Result<BenchmarkRunner> {
        match self.instrumentation_mode {
            InstrumentationType::Perf => {
                PerfInstrumentor::validate_perf_available()
                    .context("perf instrumentation requested but perf is not available")?;
            }
            InstrumentationType::Flamegraph => {
                Flamegrapher::validate_flamegraph()
                    .context("flamegraph instrumentation requested but flamegraph is not available")?;
            }
            _ => {}
        }

        // Create the BenchmarkRunner
        Ok(BenchmarkRunner {
            hook_runner: self.hook_runner,
            capture_output: self.capture_output,
            parameter_matrix: self.parameter_matrix,
            out_dir: self.out_dir,
            profile_interval: self.profile_interval,
            benchmark_cores: self.benchmark_cores,
            stop_on_log_pattern: self.stop_on_log_pattern,
            instrumentation_mode: self.instrumentation_mode,
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
        let total_runs = if self.instrumentation_mode.doubles_runs() {
            runs * 2
        } else {
            runs
        };

        info!(
            "Running benchmark: {command} for {runs} runs (commit: {commit}){}",
            self.instrumentation_mode.label()
        );

        // Run the setup script once before all benchmark runs
        self.hook_runner.run_hook(HookStage::Setup, hook_args)?;
        let mut results = Vec::with_capacity(total_runs);

        // Execute the benchmark runs
        if self.instrumentation_mode.doubles_runs() {
            // Profiling mode: alternate uninstrumented and instrumented runs
            for i in 0..runs {
                results.push(
                    self.execute_single_run(command, i * 2, commit, params, hook_args, false)?,
                );
                results.push(
                    self.execute_single_run(command, i * 2 + 1, commit, params, hook_args, true)?,
                );
            }
        } else {
            // All other modes: every run uses the configured instrumentation
            let use_instrumentation = matches!(
                self.instrumentation_mode,
                InstrumentationType::Perf | InstrumentationType::Flamegraph
            );
            for i in 0..runs {
                results.push(
                    self.execute_single_run(
                        command, i, commit, params, hook_args, use_instrumentation,
                    )?,
                );
            }
        }

        // Run the cleanup script once after all benchmark runs
        self.hook_runner.run_hook(HookStage::Cleanup, hook_args)?;

        // Calculate statistics
        let summary = ResultAnalyzer::calculate_summary(&results);

        // Create the benchmark result
        Ok(BenchmarkResult {
            command: command.to_string(),
            parameters: params.clone(),
            runs: results,
            summary,
        })
    }

    /// Execute a single benchmark run (either instrumented or uninstrumented)
    fn execute_single_run(
        &self,
        command: &str,
        iteration: usize,
        commit: &str,
        params: &HashMap<String, String>,
        hook_args: &HookArgs,
        use_instrumentation: bool,
    ) -> Result<RunResult> {
        // Create iteration-specific hook args with parameter directory
        let params_dir = ParameterUtils::params_to_dirname(params);
        let iter_args = HookArgs {
            iteration,
            params_dir: params_dir.clone(),
            ..hook_args.clone()
        };

        // Run prepare script before the benchmark run
        self.hook_runner.run_hook(HookStage::Prepare, &iter_args)?;

        let start = Instant::now();
        let (output, profile_result, flamegraph_path) = if use_instrumentation {
            match self.instrumentation_mode {
                InstrumentationType::Perf => {
                    let (output, profile, _) =
                        self.execute_command_with_perf(command, iteration, commit, params)?;
                    (output, profile, None)
                }
                InstrumentationType::Flamegraph => {
                    self.execute_command_with_flamegraph(command, iteration, commit, params)?
                }
                InstrumentationType::Profiling => {
                    let (output, profile) =
                        self.execute_command(command, iteration, commit, params)?;
                    (output, profile, None)
                }
                InstrumentationType::None => unreachable!(),
            }
        } else {
            let (output, profile) = self.execute_command(command, iteration, commit, params)?;
            (output, profile, None)
        };

        // Stop timing (if we're not profiling, otherwise the profiler takes care of timing)
        let duration = start.elapsed();
        let duration_ms = if let Some(profile) = &profile_result {
            profile.duration * 1000.0
        } else {
            duration.as_secs_f64() * 1000.0
        };

        // Determine the debug log path from the command arguments
        let debug_log_path = Self::debug_log_path(command)
            .filter(|p| p.exists());

        // Record result
        let run_result = RunResult {
            iteration,
            duration_ms,
            exit_code: output.status.code().unwrap_or(-1),
            instrumentation: if use_instrumentation {
                self.instrumentation_mode
            } else {
                InstrumentationType::None
            },
            output: if self.capture_output {
                // Only store output if explicitly requested
                Some(String::from_utf8_lossy(&output.stdout).to_string())
            } else {
                None
            },
            profile: profile_result,
            flamegraph_path,
            debug_log_path,
        };

        // Run conclude script after the benchmark run
        self.hook_runner.run_hook(HookStage::Conclude, &iter_args)?;
        Ok(run_result)
    }

    fn execute_command_with_flamegraph(
        &self,
        command: &str,
        iteration: usize,
        commit: &str,
        params: &HashMap<String, String>,
    ) -> Result<(std::process::Output, Option<ProfileResult>, Option<PathBuf>)> {
        let params_dir = ParameterUtils::params_to_dirname(params);
        let flamegraph_out_dir = self
            .out_dir
            .join(commit)
            .join(params_dir)
            .join(iteration.to_string());

        let flamegrapher = Flamegrapher::new(flamegraph_out_dir);
        let (flamegraph_command_vec, svg_path) = flamegrapher.wrap_command(command)?;
        let flamegraph_command = flamegraph_command_vec.join(" ");

        info!(
            "Executing command with flamegraph instrumentation: {}",
            flamegraph_command
        );

        let child = self.launch_command_with_affinity(&flamegraph_command)?;
        let output = child
            .wait_with_output()
            .context("Failed to wait for flamegraph command completion")?;

        let svg_created = flamegrapher.finalize_flamegraph_svg()?;
        if !svg_created {
            warn!("Flamegraph instrumentation may have failed - no SVG generated");
        }

        if !output.status.success() {
            debug!(
                "Flamegraph command failed with status: {}",
                output.status.code().unwrap_or(-1)
            );
        }

        let flamegraph_path = if svg_created { Some(svg_path) } else { None };

        Ok((output, None, flamegraph_path))
    }

    fn debug_log_path(command: &str) -> Option<PathBuf> {
        // Explicit -debuglogfile= takes priority
        if let Some(path) = command
            .split_whitespace()
            .find(|a| a.starts_with("-debuglogfile="))
            .map(|a| PathBuf::from(a.trim_start_matches("-debuglogfile=")))
        {
            return Some(path);
        }

        // Derive from -datadir= and -chain=
        let datadir = command
            .split_whitespace()
            .find(|a| a.starts_with("-datadir="))
            .map(|a| a.trim_start_matches("-datadir="))?;

        let chain = command
            .split_whitespace()
            .find(|a| a.starts_with("-chain="))
            .map(|a| a.trim_start_matches("-chain="))
            .unwrap_or("main");

        let mut path = PathBuf::from(datadir);
        // mainnet keeps debug.log directly in the datadir;
        // every other network uses a subdirectory
        if chain != "main" {
            path.push(chain);
        }
        path.push("debug.log");

        Some(path)
    }

    /// Execute a command with perf instrumentation
    fn execute_command_with_perf(
        &self,
        command: &str,
        iteration: usize,
        commit: &str,
        params: &HashMap<String, String>,
    ) -> Result<(std::process::Output, Option<ProfileResult>, Option<PathBuf>)> {
        // Create the output directory structure for this specific run
        let params_dir = ParameterUtils::params_to_dirname(params);
        let perf_out_dir = self
            .out_dir
            .join(commit)
            .join(params_dir)
            .join(iteration.to_string());

        let perf_instrumentor = PerfInstrumentor::new(perf_out_dir);
        // Wrap the command with perf
        let (perf_command_vec, perf_data_path) = perf_instrumentor.wrap_command(command)?;
        // Convert Vec<String> to a single command string for shell execution
        let perf_command = perf_command_vec.join(" ");

        info!(
            "Executing command with perf instrumentation: {}",
            perf_command
        );

        let child = self.launch_command_with_affinity(&perf_command)?;
        let output = child
            .wait_with_output()
            .context("Failed to wait for perf command completion")?;

        let perf_success = perf_instrumentor.finalize_perf_data()?;
        if !perf_success {
            warn!("perf instrumentation may have failed - no perf.data generated");
        }

        if !output.status.success() {
            debug!(
                "Perf command failed with status: {}",
                output.status.code().unwrap_or(-1)
            );
        }

        Ok((output, None, Some(perf_data_path)))
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
            self.instrumentation_mode != InstrumentationType::Profiling  && (self.capture_output || self.stop_on_log_pattern.is_some());

        // Create a command executor with our benchmark settings
        let executor = CommandExecutor::builder()
            .name(command.to_string())
            .cpu_cores(self.benchmark_cores.clone())
            .process_group(true)
            .capture_output(should_capture)
            .build()?;

        #[cfg(unix)]
        {
            executor.launch_command("sh", &["-c", command])
        }
        #[cfg(windows)]
        {
            executor.launch_command("cmd", &["/C", command])
        }
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

        // If profiling is enabled, use the profiler to execute the command
        if self.instrumentation_mode == InstrumentationType::Profiling {
            // Check for conflicts between profiling and stop_on_log_pattern
            if self.stop_on_log_pattern.is_some() {
                warn!("Both profiling and stop_on_log_pattern are configured. Profiling takes precedence, stop_on_log_pattern will be ignored for this run.");
            }
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

        if let Some(pattern) = &self.stop_on_log_pattern {
            info!("Monitoring command output for pattern: {pattern}");
            let mut monitor = LogMonitor::start_monitoring(&mut child, pattern.clone())?;
            // Wait for either pattern match or process exit
            let pattern_matched = monitor
                .wait_for_match_or_exit(&mut child, std::time::Duration::from_millis(100))?;

            if pattern_matched {
                info!("Pattern matched, terminating process");
                match child.kill() {
                    Ok(_) => debug!("Process terminated successfully"),
                    Err(e) => warn!("Failed to terminate process: {e}"),
                }

                #[cfg(unix)]
                if let Some(pgid) = child.id().checked_neg() {
                    unsafe {
                        libc::kill(pgid as i32, libc::SIGTERM);
                    }
                }
            }
        }

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
        ResultExporter::to_file(path.as_ref(), |w|
            ResultExporter::write_json(result, w))
    }

    pub fn export_json_multiple(
        results: &[BenchmarkResult],
        path: &impl AsRef<std::path::Path>,
    ) -> Result<()> {
        ResultExporter::to_file(path.as_ref(), |w|
            ResultExporter::write_json_multiple(results, w, None))
    }
}
