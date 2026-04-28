/// End-to-end hot-path performance test for the ingest service.
///
/// Sends a configurable number of HTTP requests to /logs/ingest with payloads
/// that match an existing template (so all hits go through the parse tree —
/// the LLM is not exercised). Measures HTTP latency distribution, request
/// throughput, and verifies ClickHouse delivery by row-count diff.
///
/// Env vars:
///   TOTAL_LOGS   — total log records to send (default 100000)
///   BATCH_SIZE   — logs per HTTP request (default 1; "1" = single-log mode)
///   CONCURRENCY  — concurrent in-flight requests (default 64)
///   SERVICE_URL  — ingest service URL (default http://localhost:3002)
///   CLICKHOUSE_URL — ClickHouse HTTP endpoint (default http://localhost:8123)
///   ORG_ID       — tenant id to tag the requests (default "perf")
///   STREAM_ID    — log stream id (default "perf-stream")
///
/// Run with: cargo run --release --example perf_test

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let total_logs: usize = env_or("TOTAL_LOGS", 100_000);
    let batch_size: usize = env_or("BATCH_SIZE", 1).max(1);
    let concurrency: usize = env_or("CONCURRENCY", 64);
    let service_url =
        std::env::var("SERVICE_URL").unwrap_or_else(|_| "http://localhost:3002".to_string());
    let clickhouse_url =
        std::env::var("CLICKHOUSE_URL").unwrap_or_else(|_| "http://localhost:8123".to_string());
    let org_id = std::env::var("ORG_ID").unwrap_or_else(|_| "perf".to_string());
    let stream_id = std::env::var("STREAM_ID").unwrap_or_else(|_| "perf-stream".to_string());

    let total_requests = (total_logs + batch_size - 1) / batch_size;

    println!("=== E2E Hot-Path Perf Test ===");
    println!("service:      {}", service_url);
    println!("clickhouse:   {}", clickhouse_url);
    println!("total logs:   {}", total_logs);
    println!("batch size:   {} log(s) per request", batch_size);
    println!("total reqs:   {}", total_requests);
    println!("concurrency:  {}", concurrency);
    println!("org_id:       {}", org_id);
    println!();

    let client = reqwest::Client::builder()
        .pool_max_idle_per_host(concurrency * 2)
        .timeout(Duration::from_secs(30))
        .build()?;

    // Health check
    let health = client.get(format!("{}/health", service_url)).send().await?;
    if !health.status().is_success() {
        anyhow::bail!("service /health returned {}", health.status());
    }
    let health_json: serde_json::Value = health.json().await?;
    println!("service health: {}", health_json);
    println!();

    // Baseline ClickHouse row count for this tenant
    let pre_count = ch_count_for_org(&client, &clickhouse_url, &org_id).await?;
    println!("baseline logs rows for org_id={}: {}", org_id, pre_count);
    println!();

    let semaphore = Arc::new(Semaphore::new(concurrency));
    let latencies: Arc<Mutex<Vec<Duration>>> =
        Arc::new(Mutex::new(Vec::with_capacity(total_requests)));
    let errors = Arc::new(AtomicUsize::new(0));
    let matched = Arc::new(AtomicUsize::new(0));

    println!("running...");
    let start = Instant::now();
    let mut handles = Vec::with_capacity(total_requests);

    for batch_idx in 0..total_requests {
        let permit = semaphore.clone().acquire_owned().await?;
        let client = client.clone();
        let url = format!("{}/logs/ingest", service_url);
        let latencies = latencies.clone();
        let errors = errors.clone();
        let matched = matched.clone();
        let org_id = org_id.clone();
        let stream_id = stream_id.clone();

        handles.push(tokio::spawn(async move {
            let _permit = permit;
            // Match the seed `cpu_usage: (\d+\.\d+)% - (.*)` template.
            let payload = if batch_size == 1 {
                let i = batch_idx;
                serde_json::json!({
                    "org_id": org_id,
                    "log_stream_id": stream_id,
                    "service": "perf-test",
                    "region": "local",
                    "log_stream_name": "perf-stream",
                    "message": format!("cpu_usage: {}.{}% - sample {}", i % 100, i % 1000, i),
                })
            } else {
                let logs: Vec<serde_json::Value> = (0..batch_size)
                    .map(|j| {
                        let i = batch_idx * batch_size + j;
                        serde_json::json!({
                            "org_id": org_id,
                            "log_stream_id": stream_id,
                            "service": "perf-test",
                            "region": "local",
                            "log_stream_name": "perf-stream",
                            "message": format!("cpu_usage: {}.{}% - sample {}", i % 100, i % 1000, i),
                        })
                    })
                    .collect();
                serde_json::json!({ "logs": logs })
            };

            let req_start = Instant::now();
            let result = client.post(&url).json(&payload).send().await;
            let elapsed = req_start.elapsed();

            match result {
                Ok(r) if r.status().is_success() => {
                    latencies.lock().unwrap().push(elapsed);
                    if let Ok(body) = r.json::<serde_json::Value>().await {
                        if let Some(m) = body.get("matched").and_then(|v| v.as_u64()) {
                            matched.fetch_add(m as usize, Ordering::Relaxed);
                        }
                    }
                }
                _ => {
                    errors.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }

    for h in handles {
        let _ = h.await;
    }
    let elapsed = start.elapsed();

    let mut latencies = Arc::try_unwrap(latencies)
        .map_err(|_| anyhow::anyhow!("latencies still shared"))?
        .into_inner()
        .unwrap();
    latencies.sort_unstable();

    let succeeded = latencies.len();
    let errors = errors.load(Ordering::Relaxed);
    let matched = matched.load(Ordering::Relaxed);

    let logs_succeeded = succeeded * batch_size;
    println!("\n=== Results ===");
    println!("duration:           {:.2?}", elapsed);
    println!("succeeded reqs:     {}", succeeded);
    println!("errors:             {}", errors);
    println!(
        "matched (svc):      {}  ({:.1}% of logs)",
        matched,
        100.0 * matched as f64 / logs_succeeded.max(1) as f64
    );
    println!(
        "request throughput: {:.0} req/sec",
        succeeded as f64 / elapsed.as_secs_f64()
    );
    println!(
        "log throughput:     {:.0} logs/sec  ← apples-to-apples vs raw matcher",
        logs_succeeded as f64 / elapsed.as_secs_f64()
    );
    println!();
    if !latencies.is_empty() {
        println!("HTTP latency:");
        println!("  p50:           {:?}", percentile(&latencies, 50.0));
        println!("  p90:           {:?}", percentile(&latencies, 90.0));
        println!("  p95:           {:?}", percentile(&latencies, 95.0));
        println!("  p99:           {:?}", percentile(&latencies, 99.0));
        println!("  p99.9:         {:?}", percentile(&latencies, 99.9));
        println!("  max:           {:?}", latencies.last().unwrap());
    }
    println!();

    // Allow the service's BufferedWriter time to flush (configured at 5s).
    println!("waiting 7s for ClickHouse buffered writes to flush...");
    tokio::time::sleep(Duration::from_secs(7)).await;

    let post_count = ch_count_for_org(&client, &clickhouse_url, &org_id).await?;
    let written = post_count.saturating_sub(pre_count);
    let delivery = if succeeded == 0 {
        0.0
    } else {
        100.0 * written as f64 / succeeded as f64
    };
    let empty_tid =
        ch_count_for_org_filter(&client, &clickhouse_url, &org_id, "template_id = ''").await?;

    println!("\n=== ClickHouse delivery ===");
    println!("rows landed:     {}  ({:.1}% of succeeded requests)", written, delivery);
    println!("empty template_id rows: {}  (should be 0 — invariant: no orphan rows)", empty_tid);

    Ok(())
}

fn env_or(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    let idx = ((p / 100.0) * sorted.len() as f64) as usize;
    sorted[idx.min(sorted.len() - 1)]
}

async fn ch_count_for_org(
    client: &reqwest::Client,
    base_url: &str,
    org_id: &str,
) -> anyhow::Result<u64> {
    ch_count_for_org_filter(client, base_url, org_id, "1=1").await
}

async fn ch_count_for_org_filter(
    client: &reqwest::Client,
    base_url: &str,
    org_id: &str,
    extra_where: &str,
) -> anyhow::Result<u64> {
    let q = format!(
        "SELECT count() FROM logs WHERE org_id = '{}' AND {}",
        org_id.replace('\'', "''"),
        extra_where
    );
    let resp = client
        .get(base_url)
        .query(&[("query", q.as_str())])
        .send()
        .await?
        .text()
        .await?;
    Ok(resp.trim().parse().unwrap_or(0))
}
