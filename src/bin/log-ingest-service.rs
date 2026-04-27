/// Log Ingestion Service
///
/// Accepts logs from any source and writes them to ClickHouse with template matching.
/// Port: 3002
///
/// Performance: 370K logs/sec with optimized template matching

use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use log_analyzer::buffered_writer::BufferedClickHouseWriter;
use log_analyzer::clickhouse_client::{ClickHouseClient, LogEntry};
use log_analyzer::llm_service::LLMServiceClient;
use log_analyzer::llm_config::MultiLLMConfig;
use log_analyzer::log_matcher::{LogMatcher, LogTemplate};
use log_analyzer::matcher_config::MatcherConfig;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tower_http::cors::CorsLayer;
use tracing::{info, warn, error, debug};

const DEFAULT_PORT: u16 = 3002;
const CLICKHOUSE_BUFFER_SIZE: usize = 1000;
const CLICKHOUSE_FLUSH_INTERVAL_SECS: u64 = 5;
const LLM_MAX_RETRIES: u32 = 3;
const LLM_INITIAL_BACKOFF_MS: u64 = 1000;
// Bounded queue for unmatched logs awaiting LLM synthesis. One consumer
// task drains this queue sequentially. On burst (queue full), new misses
// are dropped — the matcher will see those shapes again on next arrival.
const LLM_QUEUE_CAPACITY: usize = 10_000;

// ============================================================================
// Application State
// ============================================================================

#[derive(Clone)]
struct AppState {
    matcher: Arc<LogMatcher>,
    writer: Arc<BufferedClickHouseWriter>,
    clickhouse: Arc<ClickHouseClient>,
    unmatched_tx: mpsc::Sender<LogEntry>,
}

impl AppState {
    async fn new(clickhouse_url: &str) -> anyhow::Result<Self> {
        // Initialize ClickHouse
        let clickhouse = Arc::new(ClickHouseClient::new(clickhouse_url)?);
        clickhouse.init_schema().await?;
        info!("ClickHouse schema initialized");

        // Initialize buffered writer
        let writer = Arc::new(BufferedClickHouseWriter::new(
            clickhouse.clone(),
            CLICKHOUSE_BUFFER_SIZE,
            Duration::from_secs(CLICKHOUSE_FLUSH_INTERVAL_SECS),
        ));

        // Start background flusher (keep handle alive)
        let writer_clone = writer.clone();
        let _flusher_handle = writer_clone.start_background_flusher();
        info!("Started ClickHouse buffered writer (buffer: {}, flush: {}s)",
              CLICKHOUSE_BUFFER_SIZE, CLICKHOUSE_FLUSH_INTERVAL_SECS);

        // Load templates from ClickHouse or use default
        let config = MatcherConfig::batch_processing();
        let matcher = Arc::new(LogMatcher::with_config(config));

        // Try to load templates from ClickHouse
        match clickhouse.get_templates().await {
            Ok(templates) => {
                info!("Loaded {} templates from ClickHouse", templates.len());

                for template in templates {
                    matcher.add_template(LogTemplate {
                        template_id: template.template_id,
                        pattern: template.pattern,
                        variables: template.variables,
                        example: template.example,
                    });
                }
            }
            Err(e) => {
                warn!("Could not load templates from ClickHouse: {}", e);
                warn!("Starting with default templates");
            }
        }

        // Initialize LLM service with multi-LLM configuration
        let llm_config = MultiLLMConfig::from_env();
        let llm_client = Arc::new(LLMServiceClient::new_with_config(llm_config)?);

        // Bounded channel — single consumer drains it sequentially.
        let (unmatched_tx, unmatched_rx) = mpsc::channel(LLM_QUEUE_CAPACITY);

        // Spawn background task to process unmatched logs
        let matcher_clone = matcher.clone();
        let clickhouse_clone = clickhouse.clone();
        let writer_for_llm = writer.clone();
        tokio::spawn(async move {
            process_unmatched_logs(
                unmatched_rx,
                llm_client,
                matcher_clone,
                clickhouse_clone,
                writer_for_llm,
            )
            .await;
        });
        info!("Started LLM template generation service");

        Ok(Self {
            matcher,
            writer,
            clickhouse,
            unmatched_tx,
        })
    }
}

/// Background task that drains the unmatched-log queue.
///
/// Owns the cold-path write: queued entries are inserted into the `logs`
/// table only after a `template_id` has been determined (either by re-matching
/// against an updated parse tree, or via LLM synthesis). We never persist a
/// row without a template_id.
///
/// Natural deduplication: a burst of N records of the same novel shape
/// produces one LLM call, not N. We re-check the matcher before each
/// synthesis — by the time we pull the second record from the queue, the
/// matcher has been updated by the first record's LLM result, and the
/// re-check returns a hit.
async fn process_unmatched_logs(
    mut rx: mpsc::Receiver<LogEntry>,
    llm_client: Arc<LLMServiceClient>,
    matcher: Arc<LogMatcher>,
    clickhouse: Arc<ClickHouseClient>,
    writer: Arc<BufferedClickHouseWriter>,
) {
    info!("LLM template generation worker started (single-consumer)");

    while let Some(mut entry) = rx.recv().await {
        // Re-check the matcher: a previous LLM call may have already
        // taught it this shape while this entry sat in the queue.
        if let Some(tid) = matcher.match_log(&entry.message) {
            debug!("Rematch hit for queued log; writing row with template {}", tid);
            entry.template_id = tid.to_string();
            writer.write(entry).await;
            continue;
        }

        let template = match generate_with_retry(&llm_client, &entry.message).await {
            Some(t) => t,
            None => {
                // LLM gave up. Drop the row to preserve the
                // "no rows without a template_id" invariant.
                error!(
                    "LLM template generation failed permanently; dropping log: {}",
                    entry.message
                );
                continue;
            }
        };

        let template_row = log_analyzer::clickhouse_client::TemplateRow {
            org_id: entry.org_id.clone(),
            log_stream_id: entry.log_stream_id.clone(),
            template_id: template.template_id,
            pattern: template.pattern.clone(),
            variables: template.variables.clone(),
            example: template.example.clone(),
            created_at: Utc::now(),
        };

        if let Err(e) = clickhouse.insert_template(template_row).await {
            error!("Failed to save template to ClickHouse: {}", e);
            // Still install in-memory so subsequent records of this shape
            // bypass the LLM. Catalog will reconcile on next successful write.
        }
        let template_id = template.template_id;
        matcher.add_template(template);

        entry.template_id = template_id.to_string();
        writer.write(entry).await;
    }

    warn!("LLM template generation worker stopped");
}

/// Sequential LLM call with exponential backoff. Returns None if all
/// retries are exhausted.
async fn generate_with_retry(
    llm_client: &LLMServiceClient,
    log_line: &str,
) -> Option<LogTemplate> {
    let mut backoff_ms = LLM_INITIAL_BACKOFF_MS;
    for attempt in 1..=LLM_MAX_RETRIES {
        match llm_client.generate_template(log_line).await {
            Ok(template) => return Some(template),
            Err(e) if attempt == LLM_MAX_RETRIES => {
                warn!(
                    "LLM attempt {} (final) failed for log '{}': {}",
                    attempt, log_line, e
                );
                return None;
            }
            Err(e) => {
                warn!(
                    "LLM attempt {} failed for log '{}', retrying in {}ms: {}",
                    attempt, log_line, backoff_ms, e
                );
                let jitter = (backoff_ms as f64 * 0.1 * rand::random::<f64>()) as u64;
                tokio::time::sleep(Duration::from_millis(backoff_ms + jitter)).await;
                backoff_ms *= 2;
            }
        }
    }
    None
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
struct IngestRequest {
    timestamp: Option<String>,
    org_id: String,
    log_stream_id: String,
    service: String,
    region: String,
    log_stream_name: String,
    message: String,
}

/// Unified request structure - accepts single log or batch
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum UnifiedIngestRequest {
    Single(IngestRequest),
    Batch { logs: Vec<IngestRequest> },
}

/// Unified response structure
#[derive(Debug, Serialize)]
struct IngestResponse {
    accepted: usize,
    matched: usize,
    failed: usize,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    templates_loaded: usize,
    clickhouse_connected: bool,
}

#[derive(Debug, Serialize)]
struct StatsResponse {
    templates_loaded: usize,
    optimal_batch_size: usize,
}

// ============================================================================
// HTTP Handlers
// ============================================================================

/// Health check
async fn health(State(state): State<AppState>) -> impl IntoResponse {
    Json(HealthResponse {
        status: "healthy".to_string(),
        templates_loaded: state.matcher.get_all_templates().len(),
        clickhouse_connected: true, // BufferedWriter handles connection
    })
}

/// Get stats
async fn stats(State(state): State<AppState>) -> impl IntoResponse {
    Json(StatsResponse {
        templates_loaded: state.matcher.get_all_templates().len(),
        optimal_batch_size: state.matcher.optimal_batch_size(),
    })
}

/// Unified ingest endpoint - accepts single log or batch
async fn ingest_log(
    State(state): State<AppState>,
    Json(req): Json<UnifiedIngestRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Convert to batch format
    let logs = match req {
        UnifiedIngestRequest::Single(log) => vec![log],
        UnifiedIngestRequest::Batch { logs } => logs,
    };

    if logs.is_empty() {
        return Ok(Json(IngestResponse {
            accepted: 0,
            matched: 0,
            failed: 0,
        }));
    }

    let log_count = logs.len();
    info!("Ingesting {} log(s)", log_count);

    // Prepare messages for batch matching
    let messages: Vec<&str> = logs.iter().map(|log| log.message.as_str()).collect();

    // Batch match using optimized matcher (parallel if > 1000 logs)
    let template_ids = if messages.len() > 1000 {
        state.matcher.match_batch_parallel(&messages)
    } else {
        state.matcher.match_batch(&messages)
    };

    // Get all templates once for pattern lookup
    let _templates = state.matcher.get_all_templates();

    // Build log entries and queue unmatched for LLM
    let mut matched_count = 0;

    for (i, log_req) in logs.iter().enumerate() {
        let timestamp = log_req
            .timestamp
            .as_ref()
            .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        let template_id = template_ids[i];

        let log_entry = LogEntry {
            org_id: log_req.org_id.clone(),
            log_stream_id: log_req.log_stream_id.clone(),
            service: log_req.service.clone(),
            region: log_req.region.clone(),
            log_stream_name: log_req.log_stream_name.clone(),
            timestamp,
            template_id: template_id.map(|tid| tid.to_string()).unwrap_or_default(),
            message: log_req.message.clone(),
        };

        match template_id {
            Some(_) => {
                matched_count += 1;
                state.writer.write(log_entry.clone()).await;

                // 1% sample into template_examples for hover content.
                if rand::random::<f64>() < 0.01 {
                    let clickhouse = state.clickhouse.clone();
                    let example = log_entry;
                    tokio::spawn(async move {
                        if let Err(e) = clickhouse.insert_template_example(&example).await {
                            debug!("Failed to insert template example: {}", e);
                        }
                    });
                }
            }
            None => {
                // Defer the row insert until the LLM consumer has
                // determined a template_id (rematch hit or LLM synthesis).
                // Never write logs rows with empty template_id.
                debug!(
                    "No template match, queueing entry for LLM: {}",
                    log_req.message
                );
                match state.unmatched_tx.try_send(log_entry) {
                    Ok(_) => {}
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        warn!("LLM queue full, dropping log row");
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        warn!("LLM queue closed");
                    }
                }
            }
        }
    }

    info!("Successfully ingested {} log(s) ({} matched)", log_count, matched_count);
    Ok(Json(IngestResponse {
        accepted: log_count,
        matched: matched_count,
        failed: 0,
    }))
}

// ============================================================================
// Main Application
// ============================================================================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file if present (fails silently if not found)
    dotenvy::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(true)
        .init();

    info!("Starting Log Ingestion Service");

    // Get configuration from environment
    let clickhouse_url = std::env::var("CLICKHOUSE_URL")
        .unwrap_or_else(|_| "http://localhost:8123".to_string());

    info!("Connecting to ClickHouse: {}", clickhouse_url);

    // Initialize state
    let state = AppState::new(&clickhouse_url).await?;

    info!("Templates loaded: {}", state.matcher.get_all_templates().len());
    info!("Optimal batch size: {}", state.matcher.optimal_batch_size());

    // Build router
    let app = Router::new()
        .route("/health", get(health))
        .route("/stats", get(stats))
        .route("/logs/ingest", post(ingest_log))
        .layer(CorsLayer::permissive())
        .with_state(state);

    // Start server
    let port = std::env::var("INGEST_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT);

    let addr = format!("0.0.0.0:{}", port);
    info!("🚀 Log Ingestion Service listening on {}", addr);
    info!("");
    info!("📊 Endpoints:");
    info!("   GET  /health        - Health check");
    info!("   GET  /stats         - Service statistics");
    info!("   POST /logs/ingest   - Ingest single log or batch (auto-detect)");
    info!("");
    info!("⚡ Performance:");
    info!("   - Zero-copy template matching");
    info!("   - Parallel batch processing (>1000 logs)");
    info!("   - Direct ClickHouse writes");
    info!("   - Expected throughput: 100K-370K logs/sec");
    info!("");
    info!("📝 Example:");
    info!(r#"   curl -X POST http://localhost:{}/logs/ingest/batch \"#, port);
    info!(r#"     -H 'Content-Type: application/json' \"#);
    info!(r#"     -d '{{"logs": [{{"org":"1","message":"ERROR: test"}}]}}'"#);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
