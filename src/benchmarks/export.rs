use anyhow::{Context, Result};
use std::path::Path;

use crate::benchmarks::results::{BenchmarkResult, MasterSummary, ResultAnalyzer};

/// Functions for exporting benchmark results
pub struct ResultExporter;

impl ResultExporter {
    /// Export a single benchmark result to JSON
    pub fn export_json(result: &BenchmarkResult, path: &Path) -> Result<()> {
        let json_data = serde_json::to_string_pretty(result)
            .context("Failed to serialize benchmark results")?;

        std::fs::write(path, json_data).context("Failed to write benchmark results to file")?;

        Ok(())
    }

    /// Export multiple benchmark results to JSON, including a master summary
    pub fn export_json_multiple(results: &[BenchmarkResult], path: &Path) -> Result<()> {
        // Calculate master summary if there are multiple results
        let master_summary = if results.len() > 1 {
            ResultAnalyzer::calculate_master_summary(results)
        } else {
            None
        };

        // Create a combined structure with both results and summary
        #[derive(serde::Serialize)]
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

    /// Export benchmark results to CSV format
    pub fn export_csv(result: &BenchmarkResult, path: &Path) -> Result<()> {
        let mut file = std::fs::File::create(path)?;
        use std::io::Write;

        // header
        writeln!(file, "iteration,duration_ms,exit_code")?;
        // data rows
        for run in &result.runs {
            writeln!(
                file,
                "{},{:.2},{}",
                run.iteration, run.duration_ms, run.exit_code
            )?;
        }
        // summary
        writeln!(file)?;
        writeln!(file, "Summary:")?;
        writeln!(file, "min,{:.2}", result.summary.min)?;
        writeln!(file, "max,{:.2}", result.summary.max)?;
        writeln!(file, "mean,{:.2}", result.summary.mean)?;
        writeln!(file, "median,{:.2}", result.summary.median)?;
        writeln!(file, "std_dev,{:.2}", result.summary.std_dev)?;

        Ok(())
    }
}
