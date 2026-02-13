pub async fn create_nats_client(
    nats_url: &str,
    cert_path: Option<&std::path::PathBuf>,
    nkey_path: Option<&std::path::PathBuf>,
) -> Result<async_nats::Client, async_nats::Error> {
    let mut opts = async_nats::ConnectOptions::new();
    if let Some(p) = cert_path {
        opts = opts
            .require_tls(true)
            .add_root_certificates(p.clone());
    }

    if let Some(path) = nkey_path {
        let seed = std::fs::read_to_string(path)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to read nkey file: {}", e)))?;
        opts = opts.nkey(seed.trim().to_string());
    }

    opts.connect(nats_url).await.map_err(Into::into)
}
