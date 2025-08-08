# Hook Modes in Benchkit

Benchkit now supports multiple hook modes for different benchmarking scenarios. Hook modes control how the benchmark environment is set up and torn down.

## Available Hook Modes

### AssumeUTXO Mode (default)

- **Mode name**: `assumeutxo` or `assume_utxo`
- **Description**: Uses snapshot syncing to quickly initialize the blockchain state
- **Use case**: Fast benchmarking when you need a populated UTXO set
- **Behavior**:
  - Syncs headers from the network
  - Loads a UTXO snapshot to quickly reach a synced state
  - Suitable for benchmarks that need blockchain state but not full history

### Full IBD Mode

- **Mode name**: `full_ibd` or `fullibd`
- **Description**: Performs a full Initial Block Download without snapshots
- **Use case**: Benchmarking the full sync process or when you need complete blockchain history
- **Behavior**:
  - Only creates and clears the data directory
  - No snapshot loading
  - Bitcoin Core will perform a complete sync from genesis

## Configuration

Add the `hook_mode` field to any benchmark configuration:

```yaml
benchmarks:
  - name: "my full ibd benchmark"
    network: signet
    connect: 127.0.0.1:38333
    hook_mode: "full_ibd"  # Specify the hook mode
    benchmark:
      command: "bitcoind -datadir={datadir} -connect={connect} -chain=signet"
```

If `hook_mode` is not specified, it defaults to `assumeutxo`.

## Hook Lifecycle

Both modes share the same lifecycle stages but with different implementations:

1. **Setup**: Creates and clears the temporary data directory
1. **Prepare**:
   - AssumeUTXO: Syncs headers and loads snapshot
   - Full IBD: Only clears and recreates the data directory
1. **Conclude**: Moves debug.log to output directory and cleans data directory
1. **Cleanup**: Final cleanup of the data directory

## Adding New Hook Modes

The system is designed to be extensible. To add a new hook mode:

1. Add a new variant to the `HookMode` enum
1. Create a new executor implementing the `HookExecutor` trait
1. Update the `HookRunner::with_mode` method to handle the new mode
1. Update the `HookMode::from_str` method to parse the new mode name

Example use cases for future modes:

- Pruned node mode
- Specific network conditions simulation
- Custom initialization sequences

