use std::path::PathBuf;
use crate::networked::nats::create_nats_client;

pub async fn announce_job_async(
    job: String,
    nats_url: &str,
    cert_path: Option<&PathBuf>,
) -> Result<(), async_nats::Error> {
    let client = create_nats_client(nats_url, cert_path).await?;

    client.publish("benchkit.jobs", job.into()).await?;
    client.flush().await?;
    println!("Published to benchkit.jobs");
    Ok(())
}

pub fn announce_job(
    job: String,
    nats_url: &str,
    cert_path: Option<&PathBuf>,
) -> Result<(), async_nats::Error> {
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    runtime.block_on(announce_job_async(job, nats_url, cert_path))
}

