use anyhow::Result;
use std::io::{Write};

use crate::config::NetConfig;
use crate::networked::job::Job;
use crate::networked::nats::create_nats_client;
use crate::networked::rails::RailsApiClient;

pub async fn announce_job(
    job: &str,
    net_config: &NetConfig
) -> Result<(), async_nats::Error> {
    let nats_client = match create_nats_client(net_config.clone()).await {
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
    net_config: &NetConfig,
) -> Result<()> {
    let rails_client = RailsApiClient::new(net_config);
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

        println!("Publishing job to Rails server...");
        if let Err(e) = rails_client.post_job(&job).await {
            eprintln!("Failed to post job to Rails: {e}");
            // Optional: decide if you want to skip NATS if Rails fails
            continue; 
        }

        if let Err(e) = announce_job(&job_str, net_config).await {
            eprintln!("Failed to announce job: {e}");
            continue;
        }
    }
    Ok(())
}
