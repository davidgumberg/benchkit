#![warn(unused_extern_crates)]
use anyhow::Result;
use benchkit::{
    benchmarks,
    config::{load_app_config, parse_bench_config, load_bench_config, AppConfig, BenchmarkConfig, GlobalConfig},
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
        #[arg(short, long, required = true)]
        url: String,

        /// Certificate of the nats server the benchmark client will subscribe to.
        /// Since clients will execute arbitrary code as instructed by a publisher
        /// on the NATS server, we authenticate the server.
        #[arg(short, long)]
        crt: Option<PathBuf>,
    },
    /// Start a client that listens for benchmark jobs.
    Announce {
        #[arg(short, long, required = true)]
        benchmark_file: PathBuf,

        /// URL of the nats server the benchmark orchestrator will announce to.
        #[arg(short, long, required = true)]
        #[arg(short, long, required = true)]
        url: String,

        /// Certificate of the nats server the benchmark client will announce to.
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

    if let Commands::Networked { command } = &cli.command {
        match command  {
            NetworkedCommands::Client { out_dir, url, crt } => {
                benchkit::networked::client::client_loop(url.clone(), crt.clone(), app.clone(), out_dir.clone());
            }
            NetworkedCommands::Announce { benchmark_file, url, crt } => {
                let contents = std::fs::read_to_string(benchmark_file)
                    .expect("Failed to read benchmark file contents.");

                // String->BenchConfig->String to validate the benchmark.yml file.
                let bench_config = parse_bench_config(contents)
                    .expect("Failed to parse benchmark file contents.");
                let bench_config_str = serde_yaml::to_string(&bench_config)
                    .expect("Serialization of benchmark file failed.");

                benchkit::networked::announce::announce_job(bench_config_str, url, crt.as_ref())
                    .expect("Failed to announce job.");
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
