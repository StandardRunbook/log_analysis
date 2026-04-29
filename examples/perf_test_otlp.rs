//! OTLP/gRPC version of the e2e perf test.
//!
//! Sends batched ExportLogsServiceRequest payloads to the service's
//! OTLP gRPC LogsService. Use this for apples-to-apples comparison
//! against the JSON HTTP perf_test (same matcher, same writer, same
//! ClickHouse — only the ingest framing differs).
//!
//! Env vars:
//!   TOTAL_LOGS    — total log records to send (default 200000)
//!   BATCH_SIZE    — log records per gRPC request (default 1000)
//!   CONCURRENCY   — concurrent in-flight requests (default 32)
//!   OTLP_ENDPOINT — gRPC endpoint (default http://localhost:4317)
//!   CLICKHOUSE_URL — for delivery verification
//!   ORG_ID        — tenant id (default "perf-otlp")
//!   STREAM_ID     — log stream id (default "perf-stream")
//!
//! Run with: cargo run --release --example perf_test_otlp

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use opentelemetry_proto::tonic::collector::logs::v1::{
    logs_service_client::LogsServiceClient, ExportLogsServiceRequest,
};
use opentelemetry_proto::tonic::common::v1::{any_value::Value as AnyVal, AnyValue, KeyValue};
use opentelemetry_proto::tonic::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use opentelemetry_proto::tonic::resource::v1::Resource;
use tokio::sync::Semaphore;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let total_logs: usize = env_or("TOTAL_LOGS", 200_000);
    let batch_size: usize = env_or("BATCH_SIZE", 1_000).max(1);
    let concurrency: usize = env_or("CONCURRENCY", 32);
    let otlp_endpoint =
        std::env::var("OTLP_ENDPOINT").unwrap_or_else(|_| "http://localhost:4317".to_string());
    let clickhouse_url =
        std::env::var("CLICKHOUSE_URL").unwrap_or_else(|_| "http://localhost:8123".to_string());
    let org_id = std::env::var("ORG_ID").unwrap_or_else(|_| "perf-otlp".to_string());
    let stream_id = std::env::var("STREAM_ID").unwrap_or_else(|_| "perf-stream".to_string());

    let total_requests = total_logs.div_ceil(batch_size);

    println!("=== E2E OTLP/gRPC Perf Test ===");
    println!("otlp endpoint: {}", otlp_endpoint);
    println!("clickhouse:    {}", clickhouse_url);
    println!("total logs:    {}", total_logs);
    println!("batch size:    {} log(s) per gRPC request", batch_size);
    println!("total reqs:    {}", total_requests);
    println!("concurrency:   {}", concurrency);
    println!("org_id:        {}", org_id);
    println!();

    // Establish a single gRPC channel; tonic clients are cheap to clone
    // and share the underlying HTTP/2 connection (multiplexed).
    let endpoint = tonic::transport::Endpoint::from_shared(otlp_endpoint.clone())?
        .timeout(Duration::from_secs(30))
        .keep_alive_while_idle(true);
    let channel = endpoint.connect().await?;

    // Baseline ClickHouse row count for this tenant
    let http = reqwest::Client::new();
    let pre_count = ch_count_for_org(&http, &clickhouse_url, &org_id).await?;
    println!("baseline logs rows for org_id={}: {}", org_id, pre_count);
    println!();

    let semaphore = Arc::new(Semaphore::new(concurrency));
    let latencies: Arc<Mutex<Vec<Duration>>> =
        Arc::new(Mutex::new(Vec::with_capacity(total_requests)));
    let errors = Arc::new(AtomicUsize::new(0));

    println!("running...");
    let start = Instant::now();
    let mut handles = Vec::with_capacity(total_requests);

    for batch_idx in 0..total_requests {
        let permit = semaphore.clone().acquire_owned().await?;
        let channel = channel.clone();
        let latencies = latencies.clone();
        let errors = errors.clone();
        let org_id = org_id.clone();
        let stream_id = stream_id.clone();

        handles.push(tokio::spawn(async move {
            let _permit = permit;
            let mut client = LogsServiceClient::new(channel);

            let resource_logs = build_resource_logs(&org_id, &stream_id, batch_idx, batch_size);
            let req = ExportLogsServiceRequest {
                resource_logs: vec![resource_logs],
            };

            let req_start = Instant::now();
            let result = client.export(req).await;
            let elapsed = req_start.elapsed();

            match result {
                Ok(_) => latencies.lock().unwrap().push(elapsed),
                Err(_) => {
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
    let logs_succeeded = succeeded * batch_size;

    println!("\n=== Results ===");
    println!("duration:           {:.2?}", elapsed);
    println!("succeeded reqs:     {}", succeeded);
    println!("errors:             {}", errors);
    println!(
        "request throughput: {:.0} req/sec",
        succeeded as f64 / elapsed.as_secs_f64()
    );
    println!(
        "log throughput:     {:.0} logs/sec",
        logs_succeeded as f64 / elapsed.as_secs_f64()
    );
    println!();
    if !latencies.is_empty() {
        println!("gRPC request latency:");
        println!("  p50:           {:?}", percentile(&latencies, 50.0));
        println!("  p90:           {:?}", percentile(&latencies, 90.0));
        println!("  p95:           {:?}", percentile(&latencies, 95.0));
        println!("  p99:           {:?}", percentile(&latencies, 99.0));
        println!("  p99.9:         {:?}", percentile(&latencies, 99.9));
        println!("  max:           {:?}", latencies.last().unwrap());
    }
    println!();

    println!("waiting 7s for ClickHouse buffered writes to flush...");
    tokio::time::sleep(Duration::from_secs(7)).await;

    let post_count = ch_count_for_org(&http, &clickhouse_url, &org_id).await?;
    let written = post_count.saturating_sub(pre_count);
    let delivery = if logs_succeeded == 0 {
        0.0
    } else {
        100.0 * written as f64 / logs_succeeded as f64
    };
    let empty_tid =
        ch_count_for_org_filter(&http, &clickhouse_url, &org_id, "template_id = ''").await?;

    println!("\n=== ClickHouse delivery ===");
    println!(
        "rows landed:     {}  ({:.1}% of submitted logs)",
        written, delivery
    );
    println!(
        "empty template_id rows: {}  (should be 0 — invariant: no orphan rows)",
        empty_tid
    );

    Ok(())
}

fn build_resource_logs(
    org_id: &str,
    stream_id: &str,
    batch_idx: usize,
    batch_size: usize,
) -> ResourceLogs {
    let attrs = vec![
        kv("org_id", org_id),
        kv("log_stream_id", stream_id),
        kv("log_stream_name", "perf-otlp-stream"),
        kv("service.name", "perf-otlp"),
        kv("cloud.region", "local"),
    ];

    let log_records = (0..batch_size)
        .map(|j| {
            let i = batch_idx * batch_size + j;
            LogRecord {
                time_unix_nano: 0,
                observed_time_unix_nano: 0,
                severity_number: 0,
                severity_text: String::new(),
                body: Some(string_value(&format!(
                    "cpu_usage: {}.{}% - sample {}",
                    i % 100,
                    i % 1000,
                    i
                ))),
                attributes: vec![],
                dropped_attributes_count: 0,
                flags: 0,
                trace_id: vec![],
                span_id: vec![],
            }
        })
        .collect();

    ResourceLogs {
        resource: Some(Resource {
            attributes: attrs,
            dropped_attributes_count: 0,
        }),
        scope_logs: vec![ScopeLogs {
            scope: None,
            log_records,
            schema_url: String::new(),
        }],
        schema_url: String::new(),
    }
}

fn kv(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(string_value(value)),
    }
}

fn string_value(s: &str) -> AnyValue {
    AnyValue {
        value: Some(AnyVal::StringValue(s.to_string())),
    }
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
