use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct NetConfig {
    /// Optional url for NATS server
    pub nats_url: Option<String>,
    /// Optional NATS nkey for authentication.
    pub nkey: Option<String>,
    /// Optional path to a TLS certificate.
    pub certificate: Option<PathBuf>,
    
    /// Optional url for rails server that receives uploaded results.
    pub rails_url: Option<String>,
}
