use anyhow::{Context, Result};
use log::{debug, info};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::Instant,
};

use crate::benchmarks::hook_runner::{HookArgs, HookRunner, HookStage};
use crate::benchmarks::parameter::{ParameterList, ParameterMatrix};

/// Results from a single benchmark run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    /// The iteration number (0-indexed)
    pub iteration: usize,
    /// Duration in milliseconds
    pub duration_ms: f64,
    /// Exit code from the command
    pub exit_code: i32,
    /// Output from the command (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
}

/// Statistical summary of benchmark runs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    /// Minimum time in milliseconds
    pub min: f64,
    /// Maximum time in milliseconds
    pub max: f64,
    /// Mean time in milliseconds
    pub mean: f64,
    /// Median time in milliseconds
    pub median: f64,
    /// Standard deviation in milliseconds
    pub std_dev: f64,
}

/// Relative speed comparison between benchmark runs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeedComparison {
    /// The command/parameters this comparison is made against
    pub reference_label: String,
    /// How many times faster this benchmark is compared to the reference
    pub times_faster: f64,
    /// Standard error of the times_faster value
    pub error: f64,
}

/// Master summary comparing all benchmark runs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterSummary {
    /// The fastest benchmark command
    pub fastest_command: String,
    /// The parameters used for the fastest benchmark
    pub fastest_parameters: HashMap<String, String>,
    /// Relative speed comparisons with other benchmarks
    pub comparisons: Vec<SpeedComparison>,
}

/// Complete results from a benchmark
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    /// The command that was executed
    pub command: String,
    /// Parameters used in the command
    pub parameters: HashMap<String, String>,
    /// Results from each run
    pub runs: Vec<RunResult>,
    /// Statistical summary
    pub summary: RunSummary,
}

/// The BenchmarkRunner handles timing and execution of commands
pub struct BenchmarkRunner {
    /// Hook runner for lifecycle scripts
    hook_runner: HookRunner,
    /// Whether to capture command output
    capture_output: bool,
    /// Parameter matrix for running template commands
    parameter_matrix: Option<ParameterMatrix>,
}

impl BenchmarkRunner {
    /// Create a new BenchmarkRunner
    pub fn new(_out_dir: PathBuf, script_dir: PathBuf, capture_output: bool) -> Self {
        Self {
            hook_runner: HookRunner::new(script_dir),
            capture_output,
            parameter_matrix: None,
        }
    }

    /// Calculate a master summary for a set of benchmark results
    pub fn calculate_master_summary(results: &[BenchmarkResult]) -> Option<MasterSummary> {
        if results.is_empty() {
            return None;
        }

        // Find the fastest benchmark based on mean duration
        let fastest_idx = results
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                a.summary
                    .mean
                    .partial_cmp(&b.summary.mean)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(idx, _)| idx)
            .unwrap_or(0);

        let fastest = &results[fastest_idx];
        let fastest_mean = fastest.summary.mean;

        // Create comparisons for all other benchmarks
        let mut comparisons = Vec::new();
        for (i, result) in results.iter().enumerate() {
            if i == fastest_idx {
                continue; // Skip the fastest one (it would just be 1.0× faster than itself)
            }

            // Create a label from parameters
            let param_str = if result.parameters.is_empty() {
                "default".to_string()
            } else {
                result
                    .parameters
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<_>>()
                    .join(", ")
            };

            // Calculate how many times slower this benchmark is
            let times_slower = result.summary.mean / fastest_mean;

            // Simple error propagation (approximate)
            let relative_error_squared = (fastest.summary.std_dev / fastest_mean).powi(2)
                + (result.summary.std_dev / result.summary.mean).powi(2);
            let error = times_slower * relative_error_squared.sqrt();

            comparisons.push(SpeedComparison {
                reference_label: param_str,
                times_faster: times_slower,
                error,
            });
        }

        // Sort comparisons by slowest first (largest times_faster value)
        comparisons.sort_by(|a, b| {
            b.times_faster
                .partial_cmp(&a.times_faster)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Some(MasterSummary {
            fastest_command: fastest.command.clone(),
            fastest_parameters: fastest.parameters.clone(),
            comparisons,
        })
    }

    /// Set parameter lists for this benchmark runner
    pub fn with_parameter_lists(mut self, parameter_lists: Vec<ParameterList>) -> Self {
        self.parameter_matrix = Some(ParameterMatrix::new(&parameter_lists));
        self
    }

    /// Run a benchmark command with the specified number of runs
    pub fn run_benchmark(
        &self,
        command: &str,
        runs: usize,
        hook_args: &HookArgs,
    ) -> Result<BenchmarkResult> {
        info!("Running benchmark: {} for {} runs", command, runs);

        // Run the setup script once before all benchmark runs
        self.hook_runner.run_hook(HookStage::Setup, hook_args)?;

        let mut results = Vec::with_capacity(runs);

        for i in 0..runs {
            // Create iteration-specific hook args
            let iter_args = HookArgs {
                iteration: i,
                ..hook_args.clone()
            };

            // Run prepare script before the benchmark run
            self.hook_runner.run_hook(HookStage::Prepare, &iter_args)?;

            // Set environment variables
            let run_env = self.create_env_map(i);

            // Start timing
            let start = Instant::now();

            // Execute command
            let output = self.execute_command(command, &run_env)?;

            // Stop timing
            let duration = start.elapsed();
            let duration_ms = duration.as_secs_f64() * 1000.0;

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
            };

            results.push(run_result);

            // Run conclude script after the benchmark run
            self.hook_runner.run_hook(HookStage::Conclude, &iter_args)?;
        }

        // Run the cleanup script once after all benchmark runs
        self.hook_runner.run_hook(HookStage::Cleanup, hook_args)?;

        // Calculate statistics
        let summary = self.calculate_summary(&results);

        // Create the benchmark result
        let benchmark_result = BenchmarkResult {
            command: command.to_string(),
            parameters: HashMap::new(), // To be filled from config
            runs: results,
            summary,
        };

        Ok(benchmark_result)
    }

    /// Execute a command and capture its output
    fn execute_command(&self, command: &str, env: &HashMap<String, String>) -> Result<Output> {
        debug!("Executing command: {}", command);

        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .envs(env.iter())
            .output()
            .context("Failed to execute command")?;

        if !output.status.success() {
            debug!(
                "Command failed with status: {}",
                output.status.code().unwrap_or(-1)
            );
            // We don't return an error here because we want to capture benchmark failures
            // and include them in the results
        }

        Ok(output)
    }

    /// Create environment map for command execution
    fn create_env_map(&self, iteration: usize) -> HashMap<String, String> {
        let mut env = HashMap::new();
        env.insert("BENCHKIT_ITERATION".to_string(), iteration.to_string());
        // For compatibility
        env.insert("HYPERFINE_ITERATION".to_string(), iteration.to_string());
        env
    }

    /// Calculate summary statistics from run results
    fn calculate_summary(&self, results: &[RunResult]) -> RunSummary {
        if results.is_empty() {
            return RunSummary {
                min: 0.0,
                max: 0.0,
                mean: 0.0,
                median: 0.0,
                std_dev: 0.0,
            };
        }

        // Extract durations
        let durations: Vec<f64> = results.iter().map(|r| r.duration_ms).collect();

        // Calculate min and max
        let min = *durations
            .iter()
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap();
        let max = *durations
            .iter()
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap();

        // Calculate mean
        let mean = durations.iter().sum::<f64>() / durations.len() as f64;

        // Calculate median
        let mut sorted_durations = durations.clone();
        sorted_durations.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = if sorted_durations.len() % 2 == 0 {
            let mid = sorted_durations.len() / 2;
            (sorted_durations[mid - 1] + sorted_durations[mid]) / 2.0
        } else {
            sorted_durations[sorted_durations.len() / 2]
        };

        // Calculate standard deviation
        let variance: f64 = durations
            .iter()
            .map(|d| {
                let diff = d - mean;
                diff * diff
            })
            .sum::<f64>()
            / durations.len() as f64;
        let std_dev = variance.sqrt();

        RunSummary {
            min,
            max,
            mean,
            median,
            std_dev,
        }
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
            info!("Running command with parameters: {:?}", params);

            // Create a new hook_args with the specific commit for this parameter combination
            let mut current_hook_args = hook_args.clone();

            // Update the commit if it's in the params
            if let Some(commit) = params.get("commit") {
                current_hook_args.commit = commit.clone();
            }

            let mut result = self.run_benchmark(&command, runs, &current_hook_args)?;

            // Add parameters to the result
            result.parameters = params;

            results.push(result);
        }

        Ok(results)
    }

    /// Export results to JSON
    pub fn export_json(result: &BenchmarkResult, path: &Path) -> Result<()> {
        let json_data = serde_json::to_string_pretty(result)
            .context("Failed to serialize benchmark results")?;

        std::fs::write(path, json_data).context("Failed to write benchmark results to file")?;

        Ok(())
    }

    /// Export multiple results to JSON
    pub fn export_json_multiple(results: &[BenchmarkResult], path: &Path) -> Result<()> {
        // Calculate master summary if there are multiple results
        let master_summary = if results.len() > 1 {
            Self::calculate_master_summary(results)
        } else {
            None
        };

        // Create a combined structure with both results and summary
        #[derive(Serialize)]
        struct ExportData<'a> {
            results: &'a [BenchmarkResult],
            #[serde(skip_serializing_if = "Option::is_none")]
            master_summary: Option<MasterSummary>,
        }

        let export_data = ExportData {
            results,
            master_summary,
        };

        let json_data = serde_json::to_string_pretty(&export_data)
            .context("Failed to serialize benchmark results")?;

        std::fs::write(path, json_data).context("Failed to write benchmark results to file")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_calculate_summary() {
        let temp_dir = tempdir().unwrap();
        let runner = BenchmarkRunner::new(
            temp_dir.path().to_path_buf(),
            temp_dir.path().to_path_buf(),
            false,
        );

        let results = vec![
            RunResult {
                iteration: 0,
                duration_ms: 100.0,
                exit_code: 0,
                output: None,
            },
            RunResult {
                iteration: 1,
                duration_ms: 200.0,
                exit_code: 0,
                output: None,
            },
            RunResult {
                iteration: 2,
                duration_ms: 300.0,
                exit_code: 0,
                output: None,
            },
        ];

        let summary = runner.calculate_summary(&results);

        assert_eq!(summary.min, 100.0);
        assert_eq!(summary.max, 300.0);
        assert_eq!(summary.mean, 200.0);
        assert_eq!(summary.median, 200.0);
        assert!(summary.std_dev > 0.0);
    }
}
