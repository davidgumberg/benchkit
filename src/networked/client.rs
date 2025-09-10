use futures::StreamExt;
use tokio::sync::mpsc;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::thread;
use std::time::SystemTime;

use crate::benchmarks::Runner;
use crate::config::{parse_bench_config, AppConfig, GlobalConfig};

pub async fn listen_for_jobs(
    nats_url: &str,
    job_sender: mpsc::Sender<async_nats::Message>,
) -> Result<(), async_nats::Error> {
    let client = async_nats::connect(nats_url).await?;
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
pub fn client_loop(nats_url: String, app_config: AppConfig, out_dir: PathBuf) {
    // Create a channel for listener-executor communication.
    let (queue_sender, mut queue_receiver) = mpsc::channel::<async_nats::Message>(1024);
    
    // Spawn the listener in a dedicated thread with its own tokio runtime
    let listener_thread = thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new()
            .expect("Failed to create tokio runtime");
        
        runtime.block_on(async {
            if let Err(e) = listen_for_jobs(&nats_url, queue_sender).await {
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
    let bench = parse_bench_config(String::from_utf8(job.payload.to_vec()).unwrap()).unwrap();

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
