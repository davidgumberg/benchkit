use crate::config::NetConfig;

pub async fn create_nats_client(
    net_config: NetConfig
) -> Result<async_nats::Client, async_nats::Error> {
    let mut opts = async_nats::ConnectOptions::new();
    if let Some(p) = net_config.certificate {
        opts = opts
            .require_tls(true)
            .add_root_certificates(p.clone());
    }

    if let Some(nkey_path) = net_config.nkey {
        let seed = std::fs::read_to_string(&nkey_path)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        opts = opts.nkey(seed.trim().to_string());
    }

    opts.connect(net_config.nats_url).await.map_err(Into::into)
}
