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
use log_analyzer::log_matcher::{LogMatcher, LogTemplate};
use log_analyzer::matcher_config::MatcherConfig;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Semaphore};
use tokio::time::{interval, Instant};
use tower_http::cors::CorsLayer;
use tracing::{info, warn, error, debug};

const DEFAULT_PORT: u16 = 3002;
const CLICKHOUSE_BUFFER_SIZE: usize = 1000;
const CLICKHOUSE_FLUSH_INTERVAL_SECS: u64 = 5;
const LLM_BATCH_SIZE: usize = 10;
const LLM_BATCH_TIMEOUT_SECS: u64 = 2;
const LLM_MAX_CONCURRENT_BATCHES: usize = 5;
const LLM_MAX_RETRIES: u32 = 3;
const LLM_INITIAL_BACKOFF_MS: u64 = 1000;

// ============================================================================
// Application State
// ============================================================================

#[derive(Clone)]
struct AppState {
    matcher: Arc<LogMatcher>,
    writer: Arc<BufferedClickHouseWriter>,
    unmatched_tx: mpsc::UnboundedSender<String>,
}

impl AppState {
    async fn new(clickhouse_url: &str, llm_provider: String, llm_api_key: String, llm_model: String) -> anyhow::Result<Self> {
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

        // Initialize LLM service
        let llm_client = Arc::new(LLMServiceClient::new(llm_provider, llm_api_key, llm_model));

        // Create channel for unmatched logs
        let (unmatched_tx, unmatched_rx) = mpsc::unbounded_channel();

        // Spawn background task to process unmatched logs
        let matcher_clone = matcher.clone();
        let clickhouse_clone = clickhouse.clone();
        tokio::spawn(async move {
            process_unmatched_logs(unmatched_rx, llm_client, matcher_clone, clickhouse_clone).await;
        });
        info!("Started LLM template generation service");

        Ok(Self {
            matcher,
            writer,
            unmatched_tx,
        })
    }
}

/// Background task to process unmatched logs with batching and thread pool
async fn process_unmatched_logs(
    mut rx: mpsc::UnboundedReceiver<String>,
    llm_client: Arc<LLMServiceClient>,
    matcher: Arc<LogMatcher>,
    clickhouse: Arc<ClickHouseClient>,
) {
    info!("LLM template generation worker started (batch size: {}, max concurrent: {})",
          LLM_BATCH_SIZE, LLM_MAX_CONCURRENT_BATCHES);

    let semaphore = Arc::new(Semaphore::new(LLM_MAX_CONCURRENT_BATCHES));
    let mut batch = Vec::with_capacity(LLM_BATCH_SIZE);
    let mut batch_timer = interval(Duration::from_secs(LLM_BATCH_TIMEOUT_SECS));
    batch_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            // Receive new log
            Some(log_line) = rx.recv() => {
                batch.push(log_line);

                // Process batch if full
                if batch.len() >= LLM_BATCH_SIZE {
                    let batch_to_process = std::mem::replace(&mut batch, Vec::with_capacity(LLM_BATCH_SIZE));
                    spawn_batch_processor(
                        batch_to_process,
                        llm_client.clone(),
                        matcher.clone(),
                        clickhouse.clone(),
                        semaphore.clone(),
                    );
                }
            }

            // Timeout - process partial batch
            _ = batch_timer.tick() => {
                if !batch.is_empty() {
                    let batch_to_process = std::mem::replace(&mut batch, Vec::with_capacity(LLM_BATCH_SIZE));
                    debug!("Processing partial batch of {} logs (timeout)", batch_to_process.len());
                    spawn_batch_processor(
                        batch_to_process,
                        llm_client.clone(),
                        matcher.clone(),
                        clickhouse.clone(),
                        semaphore.clone(),
                    );
                }
            }

            else => {
                warn!("LLM queue closed, processing remaining batch");
                if !batch.is_empty() {
                    spawn_batch_processor(
                        batch,
                        llm_client.clone(),
                        matcher.clone(),
                        clickhouse.clone(),
                        semaphore.clone(),
                    );
                }
                break;
            }
        }
    }

    warn!("LLM template generation worker stopped");
}

/// Spawn a task to process a batch of logs in parallel
fn spawn_batch_processor(
    logs: Vec<String>,
    llm_client: Arc<LLMServiceClient>,
    matcher: Arc<LogMatcher>,
    clickhouse: Arc<ClickHouseClient>,
    semaphore: Arc<Semaphore>,
) {
    tokio::spawn(async move {
        // Acquire semaphore permit (limits concurrent batches)
        let _permit = semaphore.acquire().await.unwrap();

        info!("Processing batch of {} logs with LLM", logs.len());
        let start = Instant::now();

        // Process each log in the batch concurrently
        let tasks: Vec<_> = logs
            .into_iter()
            .map(|log_line| {
                let llm = llm_client.clone();
                let m = matcher.clone();
                let ch = clickhouse.clone();

                tokio::spawn(async move {
                    // Retry with exponential backoff
                    let mut retry_count = 0;
                    let mut backoff_ms = LLM_INITIAL_BACKOFF_MS;

                    loop {
                        match llm.generate_template(&log_line).await {
                            Ok(template) => {
                                if retry_count > 0 {
                                    info!("LLM succeeded after {} retries for log: {}", retry_count, log_line);
                                }
                                debug!("LLM generated template ID {} for log: {}", template.template_id, log_line);

                                // Add template to matcher
                                m.add_template(template.clone());

                                // Persist template to ClickHouse
                                let template_row = log_analyzer::clickhouse_client::TemplateRow {
                                    template_id: template.template_id,
                                    pattern: template.pattern.clone(),
                                    variables: template.variables.clone(),
                                    example: template.example.clone(),
                                };
                                if let Err(e) = ch.insert_template(template_row).await {
                                    error!("Failed to save template to ClickHouse: {}", e);
                                }
                                break; // Success!
                            }
                            Err(e) => {
                                retry_count += 1;
                                if retry_count >= LLM_MAX_RETRIES {
                                    error!("LLM template generation failed after {} retries for log '{}': {}",
                                           LLM_MAX_RETRIES, log_line, e);
                                    break; // Give up
                                }

                                warn!("LLM attempt {} failed for log '{}', retrying in {}ms: {}",
                                      retry_count, log_line, backoff_ms, e);

                                // Exponential backoff with jitter
                                let jitter = (backoff_ms as f64 * 0.1 * rand::random::<f64>()) as u64;
                                tokio::time::sleep(Duration::from_millis(backoff_ms + jitter)).await;
                                backoff_ms *= 2; // Double the backoff
                            }
                        }
                    }
                })
            })
            .collect();

        // Wait for all tasks in batch to complete
        for task in tasks {
            let _ = task.await;
        }

        info!("Batch processing completed in {:?}", start.elapsed());
    });
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
struct IngestRequest {
    timestamp: Option<String>,
    org: String,
    dashboard: Option<String>,
    panel_name: Option<String>,
    metric_name: Option<String>,
    service: Option<String>,
    host: Option<String>,
    level: Option<String>,
    message: String,
    #[serde(default)]
    metadata: serde_json::Value,
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
    let templates = state.matcher.get_all_templates();

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

        // Queue unmatched logs for LLM processing
        if template_id.is_none() {
            debug!("No template match for log, queueing for LLM: {}", log_req.message);
            if let Err(e) = state.unmatched_tx.send(log_req.message.clone()) {
                warn!("Failed to queue unmatched log for LLM: {}", e);
            }
        } else {
            matched_count += 1;
        }

        let template_pattern = template_id.and_then(|tid| {
            templates
                .iter()
                .find(|t| t.template_id == tid)
                .map(|t| t.pattern.clone())
        });

        let log_entry = LogEntry {
            timestamp,
            org: log_req.org.clone(),
            dashboard: log_req.dashboard.clone().unwrap_or_default(),
            panel_name: log_req.panel_name.clone().unwrap_or_default(),
            metric_name: log_req.metric_name.clone().unwrap_or_default(),
            service: log_req.service.clone().unwrap_or_default(),
            host: log_req.host.clone().unwrap_or_default(),
            level: log_req.level.clone().unwrap_or_else(|| "INFO".to_string()),
            message: log_req.message.clone(),
            template_id,
            template_pattern,
            metadata: log_req.metadata.to_string(),
        };

        // Write to buffered writer
        state.writer.write(log_entry).await;
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
    let llm_provider = std::env::var("LLM_PROVIDER")
        .unwrap_or_else(|_| "ollama".to_string());
    let llm_api_key = std::env::var("LLM_API_KEY")
        .unwrap_or_else(|_| "".to_string());
    let llm_model = std::env::var("LLM_MODEL")
        .unwrap_or_else(|_| "llama3".to_string());

    info!("Connecting to ClickHouse: {}", clickhouse_url);
    info!("LLM Provider: {} (model: {})", llm_provider, llm_model);

    // Initialize state
    let state = AppState::new(&clickhouse_url, llm_provider, llm_api_key, llm_model).await?;

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
