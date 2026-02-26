use anyhow::Result;
use std::path::PathBuf;
use std::io::{Write};

use crate::networked::job::Job;
use crate::networked::nats::create_nats_client;

pub async fn announce_job(
    job: &str,
    nkey_path: &PathBuf,
    nats_url: &str,
    cert_path: Option<&PathBuf>,
) -> Result<(), async_nats::Error> {
    let nats_client = match create_nats_client(nats_url, cert_path, Some(nkey_path)).await {
        Ok(client) => client,
        Err(e) => {
            eprintln!("Failed to create NATS client: {}", e);
            return Err(e.into());
        }
    };

    if let Err(e) = nats_client.publish("benchkit.jobs", job.to_string().into()).await {
        eprintln!("Failed to publish job to NATS: {}", e);
        return Err(e.into());
    }

    if let Err(e) = nats_client.flush().await {
        eprintln!("Failed to flush NATS client: {}", e);
        return Err(e.into());
    }
    println!("Published to benchkit.jobs");
    Ok(())
}

pub async fn announce_job_loop(
    nkey_path: &PathBuf,
    nats_url: &str,
    cert_path: Option<&PathBuf>,
) -> Result<()> {
    loop {
        print!("Enter benchmark file path (or 'quit' to exit): ");
        std::io::stdout().flush().expect("Failed to flush stdout");

        let mut input = String::new();
        let path_str = match std::io::stdin().read_line(&mut input) {
            Ok(n) => {
                let trimmed = input.trim();
                if n == 0 || trimmed.eq_ignore_ascii_case("quit") {
                    println!("Goodbye.");
                    break;
                }
                trimmed
            }
            Err(e) => {
                eprintln!("🤬 Goodbye: {e}");
                break;
            }
        };

        let contents = match std::fs::read_to_string(path_str) {
            Ok(contents) => contents,
            Err(e) => {
                eprintln!("Failed to read benchmark file contents: {e}");
                continue;
            }
        };

        let job = match Job::new(&contents) {
            Ok(job) => job,
            Err(e) => {
                eprintln!("Failed to create job from file contents: {e}");
                continue;
            }
        };

        let job_str = match job.to_yaml() {
            Ok(bench_config) => bench_config,
            Err(e) => {
                eprintln!("Serialization of benchmark file failed: {e}");
                continue;
            }
        };

        if let Err(e) = announce_job(&job_str, nkey_path, nats_url, cert_path).await {
            eprintln!("Failed to announce job: {e}");
            continue;
        }
    }
    Ok(())
}
