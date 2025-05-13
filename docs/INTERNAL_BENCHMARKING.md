# Internal Benchmarking

Benchkit now uses an internal benchmarking system that replaces the previous Hyperfine dependency.

## Features

- Precise timing using Rust's `std::time::Instant`
- Support for parameter substitution matrices
- Lifecycle hooks for setup, prepare, conclude, and cleanup stages
- Statistical analysis of benchmark results
- JSON results export
- Backward compatibility with existing configuration files

## Configuration

The benchmark configuration format is mostly compatible with the previous format. However, there are some changes to accommodate the new internal benchmarking system:

```yaml
global:
  # Global benchmark option defaults
  benchmark:
    warmup: 1
    runs: 5
    capture_output: false
    
  # Other global options remain the same
  wrapper: "taskset -c 1-14"
  source: $HOME/src/core/bitcoin
  scratch: $HOME/.local/state/benchkit/scratch
  commits: ["746ab19d5a13c98ae7492f9b6fb7bd6a2103c65d"]
  tmp_data_dir: /tmp/benchkit
  host: x86_64-linux-gnu

benchmarks:
  - name: "assumeutxo signet test sync"
    network: signet
    connect: 127.0.0.1:39333
    
    # Benchmark-specific options (overrides global options)
    benchmark:
      command: "bitcoind -dbcache={dbcache} -stopatheight=160001"
      warmup: 0
      runs: 1
      
      # Parameter lists for substitution
      parameter_lists:
        - var: dbcache
          values: ["450", "32000"]
```

### Options

- `warmup`: Number of warmup runs to perform (not included in results)
- `runs`: Number of measured runs to perform
- `capture_output`: Whether to capture and store command output
- `command`: The command template to execute (with parameter placeholders)
- `parameter_lists`: Lists of parameters to substitute in the command

## Lifecycle Scripts

The benchmark system executes lifecycle scripts at different stages:

1. **Setup**: Run once before all benchmark runs
2. **Prepare**: Run before each benchmark run
3. **Conclude**: Run after each benchmark run
4. **Cleanup**: Run once after all benchmark runs

These scripts now receive named arguments instead of positional arguments:

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

This makes the scripts more maintainable and self-documenting.

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

This format is compatible with existing visualization tools.