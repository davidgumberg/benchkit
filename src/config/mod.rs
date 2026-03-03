mod app;
mod benchmark;
mod net;

pub use app::*;
pub use benchmark::*;
pub use net::*;

/// Global configuration containing both app and benchmark configurations
#[derive(Debug, Clone)]
pub struct GlobalConfig {
    pub app: AppConfig,
    pub bench: BenchmarkConfig,
}
