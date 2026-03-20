use std::path::Path;

use anyhow::{Context, Result};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::benchmarks::results::BenchmarkResult;
use crate::config::NetConfig;
use crate::networked::job::Job;

#[derive(Debug, Deserialize)]
struct ResultEntry {
    id: i64,
    commit: Option<String>,
    runs: Vec<RunEntry>,
}

#[derive(Debug, Deserialize)]
struct RunEntry {
    id: i64,
    iteration: i64,
    upload_url: String,
}


pub struct RailsApiClient {
    base_url: String,
    token: String,
}

impl RailsApiClient {
    pub fn new(config: &NetConfig) -> Self {
        Self {
            base_url: config.rails_url.clone().trim_end_matches('/').to_string(),
            token: config.rails_api_token.clone(),
        }
    }

    fn api_url(&self) -> String {
        format!("{}/api/v1", self.base_url)
    }

    pub async fn post_job(&self, job: &Job) -> Result<()> {
        let client = reqwest::Client::new();
        let url = format!("{}/jobs.json", self.api_url());

        // Safely extract the first benchmark configuration
        let first_bench = job.bench.benchmarks.first()
            .context("Job must have at least one benchmark configured")?;

        let command = first_bench.benchmark.get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let warmup = first_bench.benchmark.get("warmup").and_then(|v| v.as_u64()).unwrap_or(0);
        let iterations = first_bench.benchmark.get("runs").and_then(|v| v.as_u64()).unwrap_or(1);

        let payload = json!({
            "job": {
                "uuid": job.id.to_string(),
                "name": first_bench.name,
                "source_url": job.bench.global.source.to_string_lossy(),
                "network": first_bench.network,
                "connect_node": first_bench.connect,
                "command": command,
                "commits": job.bench.global.commits,
                "warmup": warmup,
                "iterations": iterations,
                "parameter_lists": first_bench.benchmark.get("parameter_lists")
            }
        });

        let res = client.post(&url)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header(CONTENT_TYPE, "application/json")
            .json(&payload)
            .send()
            .await?;

        if !res.status().is_success() {
            anyhow::bail!("Rails rejected job: {}", res.text().await?);
        }

        Ok(())
    }

    pub fn post_result_blocking(&self, job_uuid: Uuid, result: &BenchmarkResult) -> Result<()> {
        let client = reqwest::blocking::Client::new();
        let url = format!("{}/results.json", self.api_url());

        let runs_attributes: Vec<_> = result.runs.iter().map(|run| {
            json!({
                "iteration": run.iteration,
                "duration_ms": run.duration_ms,
                "exit_code": run.exit_code
            })
        }).collect();

        let payload = json!({
            "result": {
                "job_uuid": job_uuid.to_string(),
                "command": result.command,
                "commit": result.parameters["commit"],
                "parameters": result.parameters,
                "runs_attributes": runs_attributes
            }
        });

        let res = client.post(&url)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header(CONTENT_TYPE, "application/json")
            .json(&payload)
            .send()
            .context("Failed to POST results")?;

        let status = res.status();
        let response_text = res.text().context("Failed to read response body")?;

        if !status.is_success() {
            anyhow::bail!("Rails rejected result (HTTP {}): {}", status, response_text);
        }

       let body: ResultEntry = serde_json::from_str(&response_text)
            .with_context(|| {
                format!(
                    "Failed to parse results response (HTTP {}): {}",
                    status, response_text
                )
            })?;

        for run_entry in &body.runs {
            let run_result = result
                .runs
                .iter()
                .find(|r| r.iteration as i64 == run_entry.iteration)
                .with_context(|| {
                    format!(
                        "Server returned iteration {} but no local run matches",
                        run_entry.iteration
                    )
                })?;

            if let Some(ref path) = run_result.flamegraph_path {
                if path.exists() {
                    self.upload_file(&client, &run_entry.upload_url, "flamegraph", path)?;
                }
            }

            if let Some(ref path) = run_result.debug_log_path {
                if path.exists() {
                    self.upload_file(&client, &run_entry.upload_url, "debug_log", path)?;
                }
            }
        }

        Ok(())
    }

    /// PUT a file to the per-run upload endpoint as multipart form data.
    ///
    /// The Rails endpoint (`RunsController#upload`) expects:
    ///   - `type`:  "flamegraph" | "debug_log" | "artifact"
    ///   - `file`:  the attached file
    fn upload_file(
        &self,
        client: &reqwest::blocking::Client,
        upload_url: &str,
        file_type: &str,
        path: &Path,
    ) -> Result<()> {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| file_type.to_string());

        let file_bytes = std::fs::read(path)
            .with_context(|| format!("Failed to read {}: {}", file_type, path.display()))?;

        let mime = match file_type {
            "flamegraph" => "image/svg+xml",
            "debug_log" => "text/plain",
            _ => "application/octet-stream",
        };

        let file_part = reqwest::blocking::multipart::Part::bytes(file_bytes)
            .file_name(file_name.clone())
            .mime_str(mime)?;

        let form = reqwest::blocking::multipart::Form::new()
            .text("type", file_type.to_string())
            .part("file", file_part);

        let res = client
            .put(upload_url)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .multipart(form)
            .send()
            .with_context(|| {
                format!("Failed to upload {} from {}", file_type, path.display())
            })?;

        if !res.status().is_success() {
            anyhow::bail!(
                "Upload of {} ({}) failed: {}",
                file_type,
                path.display(),
                res.text().unwrap_or_default()
            );
        }

        println!("Uploaded {}: {}", file_type, file_name);
        Ok(())
    }
}
