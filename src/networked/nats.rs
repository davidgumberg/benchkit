pub async fn create_nats_client(
    nats_url: &str,
    cert_path: Option<&std::path::PathBuf>,
) -> Result<async_nats::Client, async_nats::Error> {
    let mut opts = async_nats::ConnectOptions::new();
    if let Some(p) = cert_path {
        opts = opts
            .require_tls(true)
            .add_root_certificates(p.clone());
    }
    opts.connect(nats_url).await.map_err(Into::into)
}
