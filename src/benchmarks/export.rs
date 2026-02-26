use anyhow::{Context, Result};
use std::path::Path;
use std::io::Write;

use crate::benchmarks::results::{BenchmarkResult, MasterSummary, ResultAnalyzer};

/// Functions for exporting benchmark results
pub struct ResultExporter;

impl ResultExporter {
    /// Export a single benchmark result to JSON
    pub fn write_json(result: &BenchmarkResult, writer: &mut dyn Write) -> Result<()> {
        let json_data = serde_json::to_string_pretty(result)
            .context("Failed to serialize benchmark results")?;

        writer.write_all(json_data.as_bytes())?;
        Ok(())
    }

    /// Export multiple benchmark results to JSON, including a master summary
    pub fn write_json_multiple(results: &[BenchmarkResult], writer: &mut dyn Write) -> Result<()> {
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

        writer.write_all(json_data.as_bytes())?;
        Ok(())
    }

    /// Export benchmark results to CSV format
    pub fn write_csv(result: &BenchmarkResult, writer: &mut dyn Write) -> Result<()> {
        // header
        writeln!(writer, "iteration,duration_ms,exit_code")?;
        // data rows
        for run in &result.runs {
            writeln!(
                writer,
                "{},{:.2},{}",
                run.iteration, run.duration_ms, run.exit_code
            )?;
        }
        // summary
        writeln!(writer)?;
        writeln!(writer, "Summary:")?;
        writeln!(writer, "min,{:.2}", result.summary.min)?;
        writeln!(writer, "max,{:.2}", result.summary.max)?;
        writeln!(writer, "mean,{:.2}", result.summary.mean)?;
        writeln!(writer, "median,{:.2}", result.summary.median)?;
        writeln!(writer, "std_dev,{:.2}", result.summary.std_dev)?;

        Ok(())
    }

    // Open a file and write to it using one of the formatters.
    pub fn to_file(
        path: &Path,
        write_fn: impl FnOnce(&mut dyn Write) -> Result<()>,
    ) -> Result<()> {
        let mut file = std::fs::File::create(path)
            .with_context(|| format!("Failed to create {}", path.display()))?;
        write_fn(&mut file)?;
        file.flush()?;
        Ok(())
    }
}
