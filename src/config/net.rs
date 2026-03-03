use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct NetConfig {
    /// Optional NATS nkey for authentication.
    pub nkey: Option<String>,
    /// Optional path to a TLS certificate.
    pub certificate: Option<PathBuf>,
}
