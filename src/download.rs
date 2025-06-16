use crate::path_utils;
use crate::types::Network;
use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use log::info;
use reqwest::blocking::Client;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

#[derive(Debug)]
pub struct SnapshotInfo {
    pub network: Network,
    pub filename: &'static str,
    pub height: u32,
}

impl SnapshotInfo {
    pub fn for_network(network: &Network) -> Option<Self> {
        match network {
            Network::Mainnet => Some(Self {
                network: Network::Mainnet,
                filename: "mainnet-880000.dat",
                height: 880000,
            }),
            Network::Signet => Some(Self {
                network: Network::Signet,
                filename: "signet-160000.dat",
                height: 160000,
            }),
        }
    }
}

const SNAPSHOT_HOST: &str = "https://utxo.download/";

pub fn download_snapshot(network: &Network, snapshot_dir: &Path) -> Result<()> {
    // Make sure the snapshot directory exists
    path_utils::ensure_directory(snapshot_dir)?;

    let snapshot_info = SnapshotInfo::for_network(network)
        .ok_or_else(|| anyhow::anyhow!("No snapshot available for network {:?}", network))?;
    let filename = snapshot_info.filename;

    let url = format!("{}{}", SNAPSHOT_HOST, filename);
    let client = Client::new();
    let filepath = snapshot_dir.join(filename);
    info!("Downloading {url} to {filepath:?}");

    // Get the content length for the progress bar
    let response = client.get(&url).send()?;
    let total_size = response.content_length().unwrap_or(0);

    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:60.magenta/black}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("⟨⟨⟨⟨⟨····· "),
    );

    let mut file = File::create(&filepath)?;
    let mut downloaded = 0u64;
    let mut stream = response;

    // Stream the download in chunks
    let mut buffer = [0; 8192];
    loop {
        let bytes_read = stream.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        file.write_all(&buffer[..bytes_read])?;
        downloaded += bytes_read as u64;
        pb.set_position(downloaded);
    }

    pb.finish();
    info!("Successfully downloaded {filepath:?}");
    Ok(())
}
