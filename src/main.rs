use anyhow::Result;
use benchkit::{
    benchmarks::{self, load_bench_config, BenchmarkConfig},
    config::{load_app_config, AppConfig, GlobalConfig},
    database::{self},
    download::download_snapshot,
    system::SystemChecker,
    types::Network,
};

use clap::{Parser, Subcommand};
use env_logger::Env;
// use futures::StreamExt;
use log::{info, warn};
// use object_store::aws::{AmazonS3, AmazonS3Builder};
// use object_store::ObjectStore;
use std::{path::PathBuf, process};

const DEFAULT_CONFIG: &str = "config.yml";
const DEFAULT_BENCH_CONFIG: &str = "benchmark.yml";
// const BUCKET: &str = "benchcoin";
// const OBJECT_URL: &str = "https://hel1.your-objectstorage.com";

#[derive(Parser, Debug)]
#[command(
    version,
    about,
    long_about = "Run benchmarks using hyperfine from a YAML config"
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

    /// Run ID
    #[arg(short, long, env = "BENCH_RUN_ID")]
    run_id: Option<i64>,

    /// Pull Request number
    #[arg(short, long, env = "BENCH_PR_NUMBER")]
    pr_number: Option<i64>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Database administration
    Db {
        #[command(subcommand)]
        command: DbCommands,
    },
    /// Build bitcoin core binaries using guix
    Build {},
    /// Run benchmarks
    Run {
        #[command(subcommand)]
        command: RunCommands,
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
    S3,
}

#[derive(Subcommand, Debug)]
enum DbCommands {
    /// Initialise database if not exists
    Init,
    /// Test connection to postgres backend
    Test,
    /// [WARNING] Drop database and user
    Delete,
}

#[derive(Subcommand, Debug)]
enum RunCommands {
    /// Run all benchmarks found in config yml
    All {},
    /// Run a single benchmark from config yml
    Single {
        /// Benchmark name to run (single only)
        #[arg(short, long)]
        name: String,
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

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    let cli = Cli::parse();

    // Run system commands without loading any configuration
    if let Commands::System { command } = &cli.command {
        let checker = SystemChecker::new()?;
        match command {
            SystemCommands::Check => checker.run_checks()?,
            SystemCommands::Tune => checker.tune()?,
            SystemCommands::Reset => checker.reset()?,
        }
        process::exit(0);
    }

    // If we didn't get a run_id or pr_number generate them now.
    // The run_id in particular is used as a temporary directory for the run, collecting artifacts.
    let run_id = cli.run_id.unwrap_or_else(|| {
        let id = generate_id(false);
        warn!("No run_id specified. Generated random run_id: {}", id);
        id
    });

    let pr_number = cli.pr_number.unwrap_or_else(|| {
        let id = generate_id(true);
        warn!("No PR number specified. Generated random PR number: {}", id);
        id
    });

    let app: AppConfig = load_app_config(&cli.app_config)?;
    let bench: BenchmarkConfig = load_bench_config(&cli.bench_config, run_id, pr_number)?;
    let config = GlobalConfig { app, bench };

    match &cli.command {
        Commands::Db { command } => match command {
            DbCommands::Init => {
                database::initialize_database(&config.app.database).await?;
            }
            DbCommands::Test => {
                database::check_connection(&config.app.database).await?;
            }
            DbCommands::Delete => {
                database::delete_database_interactive(&config.app.database).await?;
            }
        },
        Commands::Build {} => {
            let builder = benchmarks::Builder::new(config.clone())?;
            builder.build()?;
        }
        Commands::Run { command } => {
            database::check_connection(&config.app.database).await?;
            let builder = benchmarks::Builder::new(config.clone())?;
            builder.build()?;
            match command {
                RunCommands::All {} => {
                    let runner = benchmarks::Runner::new(config.clone())?;
                    runner.run().await?;
                    info!("All benchmarks completed successfully.");
                }
                RunCommands::Single { name } => {
                    let runner = benchmarks::Runner::new(config.clone())?;
                    runner.run_single(name).await?;
                    info!("Benchmark completed successfully.");
                }
            }
        }
        Commands::Snapshot { command } => match command {
            SnapshotCommands::Download { network } => {
                download_snapshot(network, &config.app.snapshot_dir).await?;
            }
        },
        // Commands::S3 {} => {
        //     // Create an S3 store pointing to Hetzner
        //     let key_id = std::env::var("KEY_ID").unwrap();
        //     let secret_key = std::env::var("SECRET_ACCESS_KEY").unwrap();
        //     info!("Using:");
        //     info!("  url: {OBJECT_URL}");
        //     info!("  bucket: {BUCKET}");
        //     info!("  key_id: {key_id}");
        //     // info!("  secret_key: {secret_key}");
        //     let store = AmazonS3Builder::new()
        //         .with_bucket_name(BUCKET)
        //         .with_access_key_id(key_id)
        //         .with_secret_access_key(secret_key)
        //         .with_endpoint(OBJECT_URL)
        //         .build()?;
        //     list_files(&store).await?;
        // }
        _ => {}
    }

    Ok(())
}

// async fn list_files(store: &AmazonS3) -> anyhow::Result<()> {
//     let mut list_stream = store.list(None);
//
//     while let Some(meta) = list_stream.next().await {
//         match meta {
//             Ok(meta) => {
//                 info!("Name: {}, Size: {} bytes", meta.location, meta.size);
//             }
//             Err(e) => error!("Error listing object: {}", e),
//         }
//     }
//
//     Ok(())
// }

fn generate_id(pr: bool) -> i64 {
    use rand::Rng;
    let mut rng = rand::rng();
    if pr {
        rng.random_range(100_000_000..999_999_999)
    } else {
        rng.random_range(1000..50000)
    }
}
