// src/networked/rails.rs
use anyhow::{Context, Result};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::json;
use uuid::Uuid;

use crate::benchmarks::results::BenchmarkResult;
use crate::config::NetConfig;
use crate::networked::job::Job;

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

    pub async fn post_job(&self, job: &Job) -> Result<()> {
        let client = reqwest::Client::new();
        let url = format!("{}/jobs.json", self.base_url);

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
        let url = format!("{}/results.json", self.base_url);

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
            .send()?;

        if !res.status().is_success() {
            anyhow::bail!("Rails rejected result: {}", res.text()?);
        }

        Ok(())
    }
}
