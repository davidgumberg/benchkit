use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use crate::path_utils::process_path;

/// NetConfig as we find it in config.yml under net:
#[derive(Debug, Deserialize)]
pub struct RawNetConfig {
    pub nats_url: String,
    pub nkey: Option<PathBuf>,
    pub rails_url: String,
    pub rails_api_token: String,
    pub certificate: Option<PathBuf>,
}

impl RawNetConfig {
    pub fn finalize(self, config_dir: &Path) -> Result<NetConfig> {
        let nkey = self.nkey
            .map(|p| process_path(&p, config_dir, true))
            .transpose()?;
        let certificate = self.certificate
            .map(|p| process_path(&p, config_dir, true))
            .transpose()?;

        Ok(NetConfig {
            nats_url: self.nats_url,
            nkey,
            certificate,
            rails_url: self.rails_url,
            rails_api_token: self.rails_api_token,
        })
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct NetConfig {
    pub nats_url: String,
    pub nkey: Option<PathBuf>,
    pub certificate: Option<PathBuf>,
    pub rails_url: String,
    pub rails_api_token: String,
}
