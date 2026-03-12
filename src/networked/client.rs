use anyhow::{Context, Result};
use futures::StreamExt;
use tokio::sync::mpsc;
use std::path::PathBuf;
use std::thread;

use crate::benchmarks::Runner;
use crate::config::{AppConfig, GlobalConfig};
use crate::networked::job::Job;
use crate::networked::nats::create_nats_client;

pub async fn listen_for_jobs(
    nats_url: &str,
    nats_crt: Option<&PathBuf>,
    job_sender: mpsc::Sender<async_nats::Message>,
) -> Result<(), async_nats::Error> {
    let client = create_nats_client(nats_url, nats_crt, None).await?;

    let mut subscriber = client.subscribe("benchkit.jobs").await?;
    println!("Subscribed to benchkit.jobs");
    
    while let Some(message) = subscriber.next().await {
        println!("Received message {:?}", message);
        // Send the message to the job queue
        if job_sender.send(message).await.is_err() {
            // Receiver has been dropped, exit the loop
            eprintln!("Job processor has shut down, stopping listener");
            break;
        }
    }
    Ok(())
}

/// Set up an async thread that listens for new jobs to be announced and adds
/// them to the queue and a synchronous thread that waits for and executes
/// jobs in the queue.
pub fn client_loop(nats_url: &str, rails_url: &str, nats_crt: Option<PathBuf>, app_config: AppConfig, out_dir: PathBuf) {
    // Create a channel for listener-executor communication.
    let (queue_sender, mut queue_receiver) = mpsc::channel::<async_nats::Message>(1024);
    
    let nats_url = nats_url.to_string();
    // Spawn the listener in a dedicated thread with its own tokio runtime
    let listener_thread = thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new()
            .expect("Failed to create tokio runtime");
        
        runtime.block_on(async {
            if let Err(e) = listen_for_jobs(&nats_url, nats_crt.as_ref(), queue_sender).await {
                eprintln!("Job listener error: {}", e);
            }
        });
    });
    
    // Spawn the synchronous job processor thread
    let processor_thread = thread::spawn(move || {
        println!("Job processor started");
        
        // Processes jobs synchronously, in order
        while let Some(job) = queue_receiver.blocking_recv() {
            println!("Processing job: subject={}",
                job.subject, 
            );
            
            process_job(&job, app_config.clone(), out_dir.clone());
        }
        
        println!("Job processor shutting down");
    });
    
    listener_thread.join().expect("Listener thread panicked");
    processor_thread.join().expect("Processor thread panicked");
}

fn process_job(job_msg: &async_nats::Message, app: AppConfig, out_dir: PathBuf) -> Result<()> {
    let payload = String::from_utf8(job_msg.payload.to_vec())
        .context("job payload is not valid UTF-8")?;
    let job = Job::from_yaml(&payload)
        .context("Error building job from message payload.")?;
    // (&String::from_utf8(job_msg.payload.to_vec()).unwrap()).unwrap();
    // Use UUID for unique output dir.
    let out_dir = out_dir.join(job.id.to_string());

    let config = GlobalConfig { app, bench: job.bench };
    let runner = Runner::new(config, out_dir.clone())
        .expect("Failed to initialize job runner.");
    let results = runner.run(None, true)
        .expect("Failed to execute job runner.");

    for result in results {
        for run in result.runs {
            if run.exit_code != 0 {
                println!("Run {} of command {} failed with exit code: {}",
                    run.iteration,
                    result.command,
                    run.exit_code
                );
                // TODO: let the server know that something went wrong so someone
                // can come and fix it.
                return Ok(())
            }
        }
    }
    // TODO: upload results here.
    println!("Completed job! Find Results in {}", out_dir.display());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_listen_for_jobs_connection_error() {
        let (tx, _rx) = mpsc::channel(10);
        
        let result = listen_for_jobs("nats://255.255.255.255:9999", None, tx).await;
        
        assert!(result.is_err(), "Expected connection to fail and return an error");
    }

    #[test]
    fn test_process_job_panics_on_invalid_bytes() {
        let bad_payload = Bytes::from(vec![0, 159, 146, 150]); // Invalid UTF-8 sequence
        
        let msg = async_nats::Message {
            subject: "benchkit.jobs".into(),
            reply: None,
            payload: bad_payload,
            headers: None,
            status: None,
            description: None,
            length: 4,
        };

        let dummy_app = AppConfig::default();
        let out_dir = TempDir::new().unwrap();

        let result = process_job(&msg, dummy_app, out_dir.path().to_path_buf());
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("UTF-8"),
            "Expected UTF-8 error"
        );
    }

    #[test]
    fn test_process_job_panics_on_invalid_job_data() {
        let msg = async_nats::Message {
            subject: "benchkit.jobs".into(),
            reply: None,
            payload: Bytes::from(r#""malformed"; "yaml""#),
            headers: None,
            status: None,
            description: None,
            length: 19,
        };

        let dummy_app = AppConfig::default();
        let out_dir = TempDir::new().unwrap();

        let result = process_job(&msg, dummy_app, out_dir.path().to_path_buf());
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("job"),
            "Expected job parsing error"
        );
    }
}
