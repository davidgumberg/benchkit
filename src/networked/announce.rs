use std::path::PathBuf;
use std::io::{Write};

use crate::config::parse_bench_config;
use crate::networked::nats::create_nats_client;

pub async fn announce_job(
    job: String,
    nkey_path: &PathBuf,
    nats_url: &str,
    aws_cfg: &aws_config::SdkConfig,
    cert_path: Option<&PathBuf>,
) -> Result<(), async_nats::Error> {
    let aws_client = aws_sdk_s3::Client::new(aws_cfg);

    let nats_client = match create_nats_client(nats_url, cert_path, Some(nkey_path)).await {
        Ok(client) => client,
        Err(e) => {
            eprintln!("Failed to create NATS client: {}", e);
            return Err(e.into());
        }
    };

    if let Err(e) = nats_client.publish("benchkit.jobs", job.into()).await {
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
    aws_cfg: &aws_config::SdkConfig,
    cert_path: Option<&PathBuf>,
) -> Result<(), async_nats::Error> {
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

        let bench_config = match parse_bench_config(&contents) {
            Ok(bench_config) => bench_config,
            Err(e) => {
                eprintln!("Failed to parse benchmark file contents: {e}");
                continue;
            }
        };

        let bench_config_str = match serde_yaml::to_string(&bench_config) {
            Ok(bench_config) => bench_config,
            Err(e) => {
                eprintln!("Serialization of benchmark file failed: {e}");
                continue;
            }
        };

        announce_job(
            bench_config_str,
            nkey_path,
            nats_url,
            aws_cfg,
            cert_path,
        ).await.expect("Failed to announce job.");
    }

    Ok(())
}
