use futures::StreamExt;
use tokio::sync::mpsc;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::thread;
use std::time::SystemTime;

use crate::benchmarks::Runner;
use crate::config::{parse_bench_config, AppConfig, GlobalConfig};
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
pub fn client_loop(nats_url: &str, nats_crt: Option<PathBuf>, app_config: AppConfig, out_dir: PathBuf) {
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

fn process_job(job: &async_nats::Message, app: AppConfig, out_dir: PathBuf) {
    // Convert Bytes to String
    let bench = parse_bench_config(&String::from_utf8(job.payload.to_vec()).unwrap()).unwrap();

    // Get a hash of the job payload and system time for a unique filename.
    let mut hasher = DefaultHasher::new();
    job.payload.hash(&mut hasher) ;
    SystemTime::now().hash(&mut hasher);
    let unique_filename = format!("{:x}", hasher.finish());
    let out_dir = out_dir.join(unique_filename);

    let config = GlobalConfig { app, bench };
    let runner = Runner::new(config, out_dir.clone())
        .expect("Failed to initialize job runner.");
    runner.run(None, true)
        .expect("Failed to execute job runner.");

    println!("Completed job! Find Results in {}", out_dir.display());
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use std::env;

    // --- 1. Test Network Error Propagation ---
    // Ensures that if the NATS server is unreachable, the async function
    // doesn't hang forever and correctly bubbles up the connection error.
    #[tokio::test]
    async fn test_listen_for_jobs_connection_error() {
        let (tx, _rx) = mpsc::channel(10);
        
        // Using a definitively invalid/unroutable URL
        let result = listen_for_jobs("nats://255.255.255.255:9999", None, tx).await;
        
        assert!(result.is_err(), "Expected connection to fail and return an error");
    }

    // --- 2. Test Payload Parsing Failure (Invalid UTF-8) ---
    // The current code uses `.unwrap()` on String::from_utf8. 
    // This test ensures we explicitly know it panics on bad byte streams 
    // from the network, which kills the processor thread.
    #[test]
    #[should_panic]
    fn test_process_job_panics_on_invalid_utf8() {
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

        let dummy_app = AppConfig::default(); // Assumes AppConfig implements Default or use a mock
        let out_dir = env::temp_dir();

        process_job(&msg, dummy_app, out_dir);
    }

    #[test]
    #[should_panic]
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

        let dummy_app = AppConfig::default(); // Assumes AppConfig implements Default
        let out_dir = env::temp_dir();

        process_job(&msg, dummy_app, out_dir);
    }
}
