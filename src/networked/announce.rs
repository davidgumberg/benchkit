pub fn announce_job(
    job: String,
    nats_url: &str,
) -> Result<(), async_nats::Error> {
    let runtime = tokio::runtime::Runtime::new()
        .expect("Failed to create tokio runtime");
    runtime.block_on( async {
        let client = async_nats::connect(nats_url).await?;
        client.publish("benchkit.jobs", job.into()).await?;
        client.flush().await?;
        println!("Published to benchkit.jobs");
        Ok::<(), async_nats::Error>(())
    })?;
    
    Ok(())
}
