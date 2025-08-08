#![warn(unused_extern_crates)]
use anyhow::Result;
use benchkit::{
    benchmarks,
    config::{load_app_config, load_bench_config, AppConfig, BenchmarkConfig, GlobalConfig},
    download::download_snapshot,
    system::SystemChecker,
    types::Network,
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

        /// Output directory for storing benchmark artifacts
        #[arg(short, long, required = true)]
        out_dir: PathBuf,
    },
    /// Download an assumeutxo snapshot
    Snapshot {
        #[command(subcommand)]
        command: SnapshotCommands,
    },
    /// Check system performance settings
    System {
        #[command(subcommand)]
        command: SystemCommands,
    },
    /// Check patches apply cleanly
    Patch {
        #[command(subcommand)]
        command: PatchCommands,
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

#[derive(Subcommand, Debug)]
enum SnapshotCommands {
    /// Download a snapshot
    Download {
        /// Network (mainnet or signet)
        #[arg(value_enum)]
        network: Network,
    },
}

#[derive(Subcommand, Debug)]
enum PatchCommands {
    /// Download latest patches from GitHub
    Update {},
    /// Test the patches will apply cleanly
    Test {},
}

fn main() -> Result<()> {
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
    let bench: BenchmarkConfig = load_bench_config(&cli.bench_config)?;
    let config = GlobalConfig { app, bench };

    match &cli.command {
        Commands::Build {} => {
            let mut builder = benchmarks::Builder::new(config.clone())?;
            builder.build()?;
        }
        Commands::Run { name, out_dir } => {
            if let Some(runner_cores) = &config.bench.global.runner_cores {
                use benchkit::command::CommandExecutor;
                CommandExecutor::bind_current_process_to_cores(runner_cores)?;
            }
            let runner = benchmarks::Runner::new(config.clone(), out_dir.clone())?;
            runner.run(name.as_deref())?;
            info!(
                "{} completed successfully.",
                name.as_deref().unwrap_or("All benchmarks")
            );
        }
        Commands::Snapshot { command } => match command {
            SnapshotCommands::Download { network } => {
                download_snapshot(network, &config.app.snapshot_dir)?;
            }
        },
        Commands::Patch { command } => match command {
            PatchCommands::Test {} => {
                let mut builder = benchmarks::Builder::new(config.clone())?;
                builder.test_patch_commits()?;
            }
            PatchCommands::Update {} => {
                let builder = benchmarks::Builder::new(config.clone())?;
                builder.update_patches(true)?;
            }
        },
        _ => {}
    }

    Ok(())
}
