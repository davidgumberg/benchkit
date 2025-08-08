use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::benchmarks::profiler::ProfileResult;

/// Type of instrumentation used for a benchmark run
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InstrumentationType {
    /// Standard benchmark run without additional instrumentation
    Uninstrumented,
    /// Benchmark run under perf profiling instrumentation
    PerfInstrumented,
}

/// Results from a single benchmark run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    /// The iteration number (0-indexed)
    pub iteration: usize,
    /// Duration in milliseconds
    pub duration_ms: f64,
    /// Exit code from the command
    pub exit_code: i32,
    /// Type of instrumentation used for this run
    pub instrumentation: InstrumentationType,
    /// Output from the command (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// Profiling results (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<ProfileResult>,
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

/// Functions for analyzing benchmark results
pub struct ResultAnalyzer;

impl ResultAnalyzer {
    /// Calculate a statistical summary for benchmark run results
    pub fn calculate_summary(results: &[RunResult]) -> RunSummary {
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

        Self::calculate_summary_from_durations(&durations)
    }

    /// Calculate statistical summary from duration values
    fn calculate_summary_from_durations(durations: &[f64]) -> RunSummary {
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
        let mut sorted_durations = durations.to_vec();
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
                    .map(|(k, v)| format!("{k}={v}"))
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
}
