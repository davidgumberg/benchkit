# Benchkit

A Rust-based benchmarking toolkit designed for benchmarking Bitcoin Core, using [hyperfine](https://github.com/sharkdp/hyperfine) as the underlying benchmarking engine.

## Features

- Run single or multiple benchmarks defined in YAML configuration files
- Store benchmark results in PostgreSQL for analysis
- Support for parameterized benchmarks with multiple variable combinations
- Configurable benchmark environment variables
- Integration with CI/PR workflows via PR number and run ID tracking
- Command wrapping support (e.g., `taskset` for CPU pinning)
- System performance tuning and monitoring
- Object storage integration for benchmark artifacts
- AssumeUTXO snapshot management

## Prerequisites

- Rust 1.84.1 or later
- PostgreSQL
- hyperfine
- sudo access for database operations
- Guix (for building Bitcoin Core)

## Installation

```bash
cargo install --path .
```

## Environment Configuration

The project includes an `.envrc.example` file that shows all required environment variables. If you use `direnv`, you can copy this to `.envrc` and modify it. Otherwise, ensure these variables are set in your environment.

Key environment variables:

```bash
# Benchkit Database Configuration
export PGHOST=127.0.0.1
export PGPORT=5432
export PGDATABASE=benchmarks
export PGUSER=benchkit
export PGPASSWORD=benchpass

# Guix Build Configuration
export HOSTS=x86_64-linux-gnu  # Important: Set this to your host architecture
export SOURCES_PATH=$HOME/.local/state/guix-builds/depends-sources-cache/
export BASE_CACHE=$HOME/.local/state/guix-builds/depends-base-cache/
export SDK_PATH=$HOME/.local/state/guix-builds/macos-sdks/

# Object Storage (optional)
export KEY_ID=<your_id>
export SECRET_ACCESS_KEY=<your_password>

# Logging
export RUST_LOG=info
```

**Important Note**: When building Bitcoin Core with Guix, you should set `HOSTS` to match your target architecture. This prevents unnecessary cross-compilation.

## Command Reference

### Database Management

```bash
# Initialize a PostGres database for benchkit
benchkit db init

# Test database connection
benchkit db test

# Delete database and user (interactive)
benchkit db delete
```

### Building Bitcoin Core

```bash
# Build bitcoind binaries for specified commits
benchkit build
```

### Running Benchmarks

```bash
# Run all benchmarks from config
benchkit run all

# Run a specific benchmark
benchkit run single --name "benchmark-name"
```

### System Performance Management

```bash
# Check current system performance settings
benchkit system check

# Tune system for benchmarking (requires sudo)
benchkit system tune

# Reset system settings to default
benchkit system reset
```

### AssumeUTXO Snapshot Management

```bash
# Download snapshot for specific network
benchkit snapshot download [mainnet|signet]
```

## Configuration Files

### Application Configuration (config.yml)

```yaml
home_dir: $HOME/.local/state/benchkit
bin_dir: $HOME/.local/state/benchkit/binaries
snapshot_dir: $HOME/.local/state/benchkit/snapshots

database:
  host: localhost
  port: 5432
  database: benchmarks
  user: benchkit
  password: benchcoin
```

### Benchmark Configuration (benchmark.yml)

```yaml
global:
  hyperfine:
    warmup: 1
    runs: 5
    export_json: results.json
    shell: /bin/bash
    show_output: false

  wrapper: "taskset -c 1-14"
  source: $HOME/src/core/bitcoin
  branch: benchmark-test
  commits: ["62bd1960fdf", "e932c6168b5"]
  tmp_data_dir: /tmp/benchkit

benchmarks:
  - name: "assumeutxo signet test sync"
    network: signet
    connect: 127.0.0.1:39333
    hyperfine:
      command: "bitcoind -dbcache={dbcache} -stopatheight=160001"
      parameter_lists:
        - var: dbcache
          values: ["450", "32000"]
```

## Benchmark Scripts

The following scripts can be customized in the `scripts/` directory:

- `setup.sh`: Initial setup before benchmarking
- `prepare.sh`: Preparation before each benchmark run
- `conclude.sh`: Cleanup after each benchmark run
- `cleanup.sh`: Final cleanup after all benchmarks

## Tips

- If running against a local Bitcoin Core, it's generally easier to configure
  the "seed" node with custom `-port` and `-rpcport` settings, and then connect
  to it from the benchcoin node using `-connect=<host>:<port>`.

## Contributing

Contributions are welcome! Please ensure your code:
- Updates documentation as needed
- Follows the project's code style

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
