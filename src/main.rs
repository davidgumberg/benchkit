#![warn(unused_extern_crates)]
use anyhow::{Context, Result};
use benchkit::{
    benchmarks,
    config::{load_app_config, load_bench_config, AppConfig, BenchmarkConfig, GlobalConfig},
    system::SystemChecker,
};

use clap::{Parser, Subcommand};
use env_logger::Env;
use log::{info, warn};
use std::{path::PathBuf, process};

const DEFAULT_CONFIG: &str = "config.yml";
const DEFAULT_BENCH_CONFIG: &str = "benchmark.yml";

#[derive(Parser, Debug)]
#[command(
    version,
    about,
    long_about = "Run benchmarks for Bitcoin Core from a YAML config"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Application config
    #[arg(short, long, default_value = DEFAULT_CONFIG)]
    app_config: PathBuf,

    /// Benchmark config
    #[arg(short, long, default_value = DEFAULT_BENCH_CONFIG)]
    bench_config: PathBuf,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Build bitcoin core binaries
    Build {},
    /// Run benchmarks
    Run {
        /// Benchmark name to run (optional - runs all if not specified)
        #[arg(short, long)]
        name: Option<String>,

        /// Whether or not to build, true by default. When true, this is
        /// equivalent to running `benchkit build` and `benchkit run` with this
        /// set to false.
        #[arg(short, long, default_value = "true")]
        build: bool,

        /// Output directory for storing benchmark artifacts
        #[arg(short, long, required = true)]
        out_dir: PathBuf,
    },
    /// For orchestrating benchmarks remotely
    Networked {
        #[command(subcommand)]
        command: NetworkedCommands,
    },
    /// Check system performance settings
    System {
        #[command(subcommand)]
        command: SystemCommands,
    },
}

#[derive(Subcommand, Debug)]
enum NetworkedCommands {
    /// Start a client that listens for benchmark jobs.
    Client {
        /// Output directory for storing benchmark artifacts
        #[arg(short, long, required = true)]
        out_dir: PathBuf,

        /// URL of the nats server the benchmark client will subscribe to.
        #[arg(short, long, required = false)]
        nats_url: Option<String>,

        /// Certificate of the nats server the benchmark client will subscribe to.
        /// Only needed if using a self-signed certificate.
        #[arg(short, long)]
        crt: Option<PathBuf>,
    },
    /// Start a client that listens for benchmark jobs.
    Announce {
        /// Path to a NATS NKey (seed) file used for client authentication.
        #[arg(short = 'k', long, required = false)]
        nkey: Option<PathBuf>,

        /// URL of the nats server the benchmark orchestrator will announce to.
        #[arg(short, long, required = false)]
        nats_url: Option<String>,

        /// Certificate of the nats server the benchmark client will announce to.
        /// Only needed if using a self-signed certificate.
        #[arg(short, long)]
        crt: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
enum SystemCommands {
    /// Check current system configuration
    Check,
    /// Tune the system for benchmarking (requires sudo)
    Tune,
    /// Reset a previous tune
    Reset,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    let cli = Cli::parse();

    // Run system commands without loading any configuration
    if let Commands::System { command } = &cli.command {
        if std::env::consts::OS != "linux" {
            anyhow::bail!("System commands are only supported on Linux platforms");
        }
        let checker = SystemChecker::new()?;
        match command {
            SystemCommands::Check => checker.run_checks()?,
            SystemCommands::Tune => checker.tune()?,
            SystemCommands::Reset => checker.reset()?,
        }
        process::exit(0);
    }

    let app: AppConfig = load_app_config(&cli.app_config)?;

    if let Commands::Networked { command } = &cli.command {
        match command  {
            NetworkedCommands::Client { out_dir, nats_url, crt } => {
                let resolved_crt = crt
                    .clone()
                    .or_else(|| app.net.as_ref().and_then(|n| n.certificate.clone()));


                let resolved_nats_url = nats_url
                    .clone()
                    .or_else(|| app.net.as_ref().and_then(|n| n.nats_url.clone()))
                    .context("nats_url is required: provide it via --nats-url or in the --config file.")?;

                benchkit::networked::client::client_loop(&resolved_nats_url, resolved_crt, app.clone(), out_dir.clone());
            }
            NetworkedCommands::Announce { nkey, nats_url, crt } => {
                let resolved_nkey = nkey
                    .clone()
                    .or_else(|| app.net.as_ref().and_then(|n| n.nkey.as_ref().map(PathBuf::from)))
                    .context("nkey is required: provide it via --nkey or in the --config file.")?;

                let resolved_crt = crt
                    .clone()
                    .or_else(|| app.net.as_ref().and_then(|n| n.certificate.clone()));

                let resolved_nats_url = nats_url
                    .clone()
                    .or_else(|| app.net.as_ref().and_then(|n| n.nats_url.clone()))
                    .context("nats_url is required: provide it via --nats-url or in the --config file.")?;

                benchkit::networked::announce::announce_job_loop(&resolved_nkey, &resolved_nats_url, resolved_crt.as_ref()).await.expect("Error starting announce loop.");
            }
        }
    }

    let bench: BenchmarkConfig = load_bench_config(&cli.bench_config)?;
    let config = GlobalConfig { app, bench };

    match &cli.command {
        Commands::Build {} => {
            let mut builder = benchmarks::Builder::new(config.clone())?;
            builder.build()?;
        }
        Commands::Run { name, out_dir, build } => {
            if let Some(runner_cores) = &config.bench.global.runner_cores {
                use benchkit::command::CommandExecutor;
                CommandExecutor::bind_current_process_to_cores(runner_cores)?;
            }
            let runner = benchmarks::Runner::new(config.clone(), out_dir.clone())?;
            runner.run(name.as_deref(), *build)?;
            info!(
                "{} completed successfully.",
                name.as_deref().unwrap_or("All benchmarks")
            );
        }
        _ => {}
    }

    Ok(())
}
