use anyhow::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::{BenchmarkConfig, parse_bench_config};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Job {
    pub id: Uuid,
    pub bench: BenchmarkConfig, // Todo: jobs might need their own config format.
}

impl Job {
    pub fn new(bench_config: &str) -> Result<Self> {
        let bench: BenchmarkConfig = parse_bench_config(bench_config)?;
        let id = Uuid::new_v4();
        Ok(Job { bench, id })
    }

    pub fn from_yaml(yaml: &str) -> Result<Self> {
        Ok(serde_yaml::from_str(yaml)?)
    }
    pub fn to_yaml(&self) -> Result<String> {
        Ok(serde_yaml::to_string(self)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: a minimal valid YAML that matches BenchmarkConfig.
    // Adjust fields to the real shape of BenchmarkConfig.
    const VALID_YAML: &str = r#"---
global:
  source: https://github.com/bitcoin/bitcoin.git
  commits: ["e221b252465", "8f73d952214"]

benchmarks:
  - name: "signet test sync"
    network: signet
    connect: 127.0.0.1:39333
    benchmark:
      command: "bitcoind -dbcache={dbcache} -stopatheight=10000"
      warmup: 0
"#;

    #[test]
    fn creates_job_successfully() {
        let job = Job::new(VALID_YAML).expect("Job should be created");
        assert_eq!(job.bench.benchmarks[0].name, "signet test sync");
        assert_eq!(job.bench.benchmarks[0].network, "signet");
        assert_ne!(job.id, Uuid::nil());
    }

    #[test]
    fn returns_error_on_missing_fields() {
        let incomplete_yaml = r#"---
global:
  source: https://github.com/bitcoin/bitcoin.git
"#;
        let result = Job::new(incomplete_yaml);
        assert!(result.is_err(), "Expected an error for missing fields");
    }
}
