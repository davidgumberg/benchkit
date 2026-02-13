use std::path::PathBuf;

pub async fn announce_job_async(
    job: String,
    nats_url: &str,
    cert_path: Option<PathBuf>,
) -> Result<(), async_nats::Error> {
    let mut opts = async_nats::ConnectOptions::new();
    if let Some(p) = cert_path {
        opts = opts
            .require_tls(true)
            .add_root_certificates(PathBuf::from(p));
    }

    let client = opts.connect(nats_url).await?;
    client.publish("benchkit.jobs", job.into()).await?;
    client.flush().await?;
    println!("Published to benchkit.jobs");
    Ok(())
}

pub fn announce_job(
    job: String,
    nats_url: &str,
    cert_path: Option<PathBuf>,
) -> Result<(), async_nats::Error> {
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    runtime.block_on(announce_job_async(job, nats_url, cert_path))
}
