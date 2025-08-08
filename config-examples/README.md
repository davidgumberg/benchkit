# Benchkit Configuration Examples

This directory contains example configuration files demonstrating various benchkit features. Each file is a complete, runnable configuration focused on a specific feature.

## Examples

### `assumeutxo.yml`

Demonstrates the default AssumeUTXO mode, which uses snapshot syncing to quickly initialize the blockchain state. This is the fastest way to get to a synced state for benchmarking.

### `full-ibd.yml`

Shows Full IBD (Initial Block Download) mode, which performs a complete sync from genesis without snapshots. Useful for benchmarking the full sync process.

### `stop-on-log-pattern.yml`

Demonstrates using regex patterns to stop benchmarks when specific log messages are detected, allowing precise control over when benchmarks end.

### `parameter-matrix.yml`

Shows how to use parameter matrices to run benchmarks with different combinations of parameters, creating comprehensive benchmark suites.

## Hook Modes

Benchkit supports different hook modes that control how the benchmark environment is prepared:

### AssumeUTXO Mode (`assumeutxo`)

- **Default mode** - used when `mode` field is omitted
- Uses snapshot syncing to quickly reach a synced blockchain state
- Syncs headers from network, then loads UTXO snapshot before restarting with background sync disabled
- Best for benchmarks that want to "benchmark the interesting stuff" (near the chain tip)

### Full IBD Mode (`full_ibd`)

- Performs complete Initial Block Download from genesis
- No snapshot loading - creates fresh, empty data directory
- Useful for benchmarking the full sync process

## Running Examples

```bash
# Run all benchmarks in the configuration
benchkit --bench-config <path_to_config> run all --out_dir ./results

# Run a specific benchmark by name
benchkit --bench-config <path_to_config> run single --name "benchmark name" --out_dir ./results
```

Make sure to update the `source` path and `commits` in the global section to match your setup!

## Creating Custom Configurations

1. Start with the example closest to your use case
1. Modify the `global` section for your environment
1. Add or modify benchmarks in the `benchmarks` section
1. Use parameter matrices to test multiple configurations efficiently
1. Consider which hook mode is appropriate for your benchmarking goals

