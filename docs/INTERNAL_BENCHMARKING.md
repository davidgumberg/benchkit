# Internal Benchmarking

Benchkit now uses an internal benchmarking system that replaces the previous Hyperfine dependency.

## Features

- Uses Rust's `std::time::Instant`
- Retains support for parameter substitution matrices
- Retains Hyperfine-easue "hooks" for setup, prepare, conclude, and cleanup stages
- CPU affinity control for consistent benchmarking
- Basic statistical analysis of benchmark results
- JSON results export

## Configuration

```yaml
global:
  # Global benchmark option defaults
  benchmark:
    warmup: 1
    runs: 5
    capture_output: false

  # CPU affinity control
  benchmark_cores: "1-7"    # Cores to run benchmark commands on
  runner_cores: "0"         # Core to bind the main benchkit process to

  # Other global options remain the same
  source: $HOME/src/core/bitcoin
  scratch: $HOME/.local/state/benchkit/scratch
  commits: ["746ab19d5a13c98ae7492f9b6fb7bd6a2103c65d"]
  tmp_data_dir: /tmp/benchkit

benchmarks:
  - name: "assumeutxo signet test sync"
    network: signet
    connect: 127.0.0.1:39333

    # Benchmark-specific options (overrides global options)
    benchmark:
      command: "bitcoind -dbcache={dbcache} -stopatheight=170000"
      warmup: 0
      runs: 1

      # Parameter lists for substitution
      parameter_lists:
        - var: dbcache
          values: ["450", "32000"]
```

### Benchmark Options

- `warmup`: Number of warmup runs to perform (not included in results)
- `runs`: Number of measured runs to perform
- `capture_output`: Whether to capture and store command output
- `command`: The command template to execute (with parameter placeholders)
- `parameter_lists`: Lists of parameters to substitute in the command

### CPU Affinity Options

- `benchmark_cores`: CPU cores to run benchmark commands on (e.g., "1-7", "0,2,4-6")
- `runner_cores`: CPU core(s) to bind the main benchkit process to (e.g., "0")

This CPU affinity control replaces the previous `wrapper` command approach using `taskset`.

## Lifecycle Scripts

The benchmark system executes lifecycle scripts at different stages, in a *very* Hyperfine-insipred way:

1. **Setup**: Run once before all benchmark runs
1. **Prepare**: Run before each benchmark run
1. **Conclude**: Run after each benchmark run
1. **Cleanup**: Run once after all benchmark runs

These scripts receive named arguments:

```
--binary=/path/to/binary
--connect=127.0.0.1:8333
--network=signet
--out-dir=/path/to/output
--snapshot=/path/to/snapshot
--datadir=/path/to/data
--iteration=0
--commit=abcdef
```

## Parameter Substitution

The parameter substitution system allows you to run benchmarks with different combinations of parameters. For example:

```yaml
command: "bitcoind -dbcache={dbcache} -stopatheight={height}"
parameter_lists:
  - var: dbcache
    values: ["450", "32000"]
  - var: height
    values: ["100000", "200000"]
```

This will run the benchmark with all combinations of `dbcache` and `height` values.

## Results

Benchmark results are exported in JSON format, containing:

- Command executed
- Parameters used
- Results from each run (time, exit code, etc.)
- Statistical summary (min, max, mean, median, standard deviation)
- Master summary with relative speed comparisons (when running multiple parameter combinations)

### Master Summary

When running benchmarks with multiple parameter combinations, a master summary section is included in the results. This summary provides:

- The fastest benchmark configuration identified
- Relative speed comparisons between each configuration and the fastest one
- Error margins for the comparisons

This is similar to how Hyperfine displays results like:

```
Summary
  bitcoind -dbcache=32000 ran
    1.75 ± 0.08 times faster than bitcoind -dbcache=450
```

The master summary helps quickly identify which configuration performed best and by what margin.
