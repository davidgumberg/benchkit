# Benchkit

A Rust-based benchmarking toolkit designed for benchmarking Bitcoin Core, using
[hyperfine](https://github.com/sharkdp/hyperfine) as the underlying
benchmarking engine.

## Features

- Run single or multiple benchmarks defined in YAML configuration files
- Store benchmark results in PostgreSQL for analysis
- Support for parameterized benchmarks with multiple variable combinations
- Configurable benchmark environment variables
- Integration with CI/PR workflows via PR number and run ID tracking
- Command wrapping support (e.g., `taskset` for CPU pinning)

## Prerequisites

- Rust 1.84.1 or later
- PostgreSQL
- hyperfine
- sudo access for database operations

## Installation

```bash
cargo install --path .
```

## Usage

### Database Setup

Test database connection:
```bash
benchkit db test
```

Initialize the database:
```bash
benchkit db init
```

Delete database and user (caution):
```bash
benchkit db delete
```

### Build Bitcoin Core

Build bitcoind binaries:
```bash
benchkit build
```

### Running Benchmarks

Run all benchmarks from config:
```bash
benchkit run all
```

Run a specific benchmark:
```bash
benchkit run single --name "benchmark-name"
```

### Configuration

Benchkit uses YAML configuration files to define it's own settings and configure benchmarks.

A typical benchkit *config.yml* looks like:

```yaml
---
# Benchkit home directory
home_dir: $HOME/.local/state/benchkit

# The directory intermediate built binaries will be saved to.
bin_dir: $HOME/.local/state/benchkit/binaries

# Database configuration
database:

  # postgres host
  host: localhost

  # postgres port
  port: 5432

  # database name
  database: benchmarks

  # postgres username
  user: benchkit

  # postgres password
  password: benchcoin
```

And a *benchmark.yml*:

```yaml
---
# Global benchmarking options.
global:

  # Global hyperfine option defaults.
  # Will be overwritten by local options specified per-benchmark.
  hyperfine:
    warmup: 1
    runs: 5
    export_json: results.json
    shell: /bin/bash
    show_output: false

  # An optional command to wrap the hyperfine command.
  wrapper: "taskset -c 1-14"

  # Path to source code (required).
  # Can point to a local or online fork of bitcoin/bitcoin.
  source: $HOME/src/core/bitcoin

  # Which branch of the source to check out (required).
  branch: benchmark-test

  # Commits to build binaries from (required).
  # A list of one or more all found in <branch>
  commits: ["62bd1960fdf", "e932c6168b5"]

# Local benchmark config.
benchmarks:

  # benchmark name (required).
  - name: "Check bitcoind version"

    # Bitcoin network to run on (main, test, testnet4, signet, regtest)
    network: signet

    # Local hyperfine options.
    # These override global hyperfine options in case of conflict.
    # Uses regular hyperfine syntax.
    hyperfine:

      # The correct binary for the [commit] will be substituted and the (bitcoin) [network] applied automatically.
      # {dbcache} is an explicit (additional) parameterisation from [parameter_lists] below.
      command: "bitcoind -dbcache={dbcache} --version"
      warmup: 5
      runs: 10

      # These have "sane" defaults in benchkit, but can point to any command or script too
      # setup:
      # conclude:
      # prepare:
      # cleanup:

      # A list of zero or more parameters.
      # These will be tried as a matrix.
      parameter_lists:

        # The variable name to use in hyperfine command substitution.
        - var: dbcache
          # A list of values to substitute in.
          values: ["450", "32000"]

```

## Contributing

Contributions are welcome! Please ensure your code:
- Updates documentation as needed
- Follows the project's code style

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
