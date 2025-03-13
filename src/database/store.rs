use anyhow::{Context, Result};
use postgres::{Client, NoTls};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct BenchmarkResult {
    command: String,
    mean: f64,
    stddev: Option<f64>,
    median: f64,
    user: f64,
    system: f64,
    min: f64,
    max: f64,
    times: Vec<f64>,
    exit_codes: Vec<i32>,
}

#[derive(Deserialize)]
struct Results {
    results: Vec<BenchmarkResult>,
}

pub fn store_results(db_url: &str, bench_name: &str, result_json: &str, run_id: i64) -> Result<()> {
    let mut client = Client::connect(db_url, NoTls)?;

    let results: Results = serde_json::from_str(result_json)
        .with_context(|| "Failed to parse benchmark results JSON")?;

    for result in &results.results {
        store_benchmark_result(&mut client, bench_name, result, run_id)?;
    }

    Ok(())
}

fn store_benchmark_result(
    client: &mut Client,
    bench_name: &str,
    result: &BenchmarkResult,
    run_id: i64,
) -> Result<()> {
    let benchmark_id = insert_benchmark(client, bench_name, result, run_id)?;
    let run_id = insert_benchmark_run(client, benchmark_id, result)?;
    insert_measurements(client, run_id, result)?;

    Ok(())
}

fn insert_benchmark(
    client: &mut Client,
    bench_name: &str,
    result: &BenchmarkResult,
    run_id: i64,
) -> Result<i32> {
    let benchmark_id: i32 = client
        .query_one(
            "INSERT INTO benchmarks (name, command, run_id)
            VALUES ($1, $2, $3::bigint, $4::bigint) RETURNING id",
            &[&bench_name, &result.command, &run_id],
        )
        .with_context(|| "Failed to insert benchmark")?
        .get(0);

    Ok(benchmark_id)
}

fn insert_benchmark_run(
    client: &mut Client,
    benchmark_id: i32,
    result: &BenchmarkResult,
) -> Result<i32> {
    let run_id: i32 = client
        .query_one(
            "INSERT INTO runs (
                benchmark_id, mean, stddev, median, user_time,
                system_time, min_time, max_time
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
            &[
                &benchmark_id,
                &result.mean,
                &result.stddev,
                &result.median,
                &result.user,
                &result.system,
                &result.min,
                &result.max,
            ],
        )
        .with_context(|| "Failed to insert benchmark run")?
        .get(0);

    Ok(run_id)
}

fn insert_measurements(client: &mut Client, run_id: i32, result: &BenchmarkResult) -> Result<()> {
    for (idx, (time, exit_code)) in result
        .times
        .iter()
        .zip(result.exit_codes.iter())
        .enumerate()
    {
        client
            .execute(
                "INSERT INTO measurements (
                benchmark_run_id, execution_time, exit_code, measurement_order
            ) VALUES ($1, $2, $3, $4)",
                &[&run_id, time, exit_code, &(idx as i32)],
            )
            .with_context(|| "Failed to insert measurement")?;
    }

    Ok(())
}
