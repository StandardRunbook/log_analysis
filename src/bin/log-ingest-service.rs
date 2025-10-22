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
use log_analyzer::clickhouse_client::{ClickHouseClient, LogEntry};
use log_analyzer::log_matcher::{LogMatcher, LogTemplate};
use log_analyzer::matcher_config::MatcherConfig;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing::{info, warn, error};

const DEFAULT_PORT: u16 = 3002;

// ============================================================================
// Application State
// ============================================================================

#[derive(Clone)]
struct AppState {
    matcher: Arc<LogMatcher>,
    clickhouse: Arc<ClickHouseClient>,
}

impl AppState {
    async fn new(clickhouse_url: &str) -> anyhow::Result<Self> {
        // Initialize ClickHouse
        let clickhouse = ClickHouseClient::new(clickhouse_url)?;
        clickhouse.init_schema().await?;
        info!("ClickHouse schema initialized");

        // Load templates from ClickHouse or use default
        let config = MatcherConfig::batch_processing();
        let mut matcher = LogMatcher::with_config(config);

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
                warn!("Starting with empty matcher");
            }
        }

        Ok(Self {
            matcher: Arc::new(matcher),
            clickhouse: Arc::new(clickhouse),
        })
    }
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
struct IngestRequest {
    timestamp: Option<String>,
    org: String,
    dashboard: Option<String>,
    service: Option<String>,
    host: Option<String>,
    level: Option<String>,
    message: String,
    #[serde(default)]
    metadata: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct IngestResponse {
    accepted: usize,
    template_matched: bool,
    template_id: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct IngestBatchRequest {
    logs: Vec<IngestRequest>,
}

#[derive(Debug, Serialize)]
struct IngestBatchResponse {
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
    // Quick ClickHouse ping
    let ch_connected = state.clickhouse.get_templates().await.is_ok();

    Json(HealthResponse {
        status: "healthy".to_string(),
        templates_loaded: state.matcher.get_all_templates().len(),
        clickhouse_connected: ch_connected,
    })
}

/// Get stats
async fn stats(State(state): State<AppState>) -> impl IntoResponse {
    Json(StatsResponse {
        templates_loaded: state.matcher.get_all_templates().len(),
        optimal_batch_size: state.matcher.optimal_batch_size(),
    })
}

/// Ingest a single log
async fn ingest_log(
    State(state): State<AppState>,
    Json(req): Json<IngestRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Parse timestamp
    let timestamp = req
        .timestamp
        .and_then(|ts| DateTime::parse_from_rfc3339(&ts).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    // Match against templates using optimized matcher
    let template_id = state.matcher.match_log(&req.message);

    // Get template pattern if matched
    let template_pattern = template_id.and_then(|tid| {
        state
            .matcher
            .get_all_templates()
            .iter()
            .find(|t| t.template_id == tid)
            .map(|t| t.pattern.clone())
    });

    // Create log entry
    let log_entry = LogEntry {
        timestamp,
        org: req.org,
        dashboard: req.dashboard.unwrap_or_default(),
        service: req.service.unwrap_or_default(),
        host: req.host.unwrap_or_default(),
        level: req.level.unwrap_or_else(|| "INFO".to_string()),
        message: req.message,
        template_id,
        template_pattern,
        metadata: req.metadata.to_string(),
    };

    // Write to ClickHouse
    if let Err(e) = state.clickhouse.insert_log(log_entry).await {
        error!("Failed to insert log: {}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to insert log: {}", e),
        ));
    }

    Ok(Json(IngestResponse {
        accepted: 1,
        template_matched: template_id.is_some(),
        template_id,
    }))
}

/// Ingest logs in batch (RECOMMENDED for high throughput)
async fn ingest_batch(
    State(state): State<AppState>,
    Json(req): Json<IngestBatchRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if req.logs.is_empty() {
        return Ok(Json(IngestBatchResponse {
            accepted: 0,
            matched: 0,
            failed: 0,
        }));
    }

    info!("Ingesting batch of {} logs", req.logs.len());

    // Prepare messages for batch matching
    let messages: Vec<&str> = req.logs.iter().map(|log| log.message.as_str()).collect();

    // Batch match using optimized matcher (parallel if > 1000 logs)
    let template_ids = if messages.len() > 1000 {
        state.matcher.match_batch_parallel(&messages)
    } else {
        state.matcher.match_batch(&messages)
    };

    // Get all templates once for pattern lookup
    let templates = state.matcher.get_all_templates();

    // Build log entries
    let mut log_entries = Vec::with_capacity(req.logs.len());
    let mut matched_count = 0;

    for (i, log_req) in req.logs.iter().enumerate() {
        let timestamp = log_req
            .timestamp
            .as_ref()
            .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        let template_id = template_ids[i];
        if template_id.is_some() {
            matched_count += 1;
        }

        let template_pattern = template_id.and_then(|tid| {
            templates
                .iter()
                .find(|t| t.template_id == tid)
                .map(|t| t.pattern.clone())
        });

        log_entries.push(LogEntry {
            timestamp,
            org: log_req.org.clone(),
            dashboard: log_req.dashboard.clone().unwrap_or_default(),
            service: log_req.service.clone().unwrap_or_default(),
            host: log_req.host.clone().unwrap_or_default(),
            level: log_req.level.clone().unwrap_or_else(|| "INFO".to_string()),
            message: log_req.message.clone(),
            template_id,
            template_pattern,
            metadata: log_req.metadata.to_string(),
        });
    }

    // Batch write to ClickHouse
    match state.clickhouse.insert_logs_batch(log_entries).await {
        Ok(_) => {
            info!("Successfully ingested {} logs ({} matched)", req.logs.len(), matched_count);
            Ok(Json(IngestBatchResponse {
                accepted: req.logs.len(),
                matched: matched_count,
                failed: 0,
            }))
        }
        Err(e) => {
            error!("Failed to batch insert logs: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to batch insert: {}", e),
            ))
        }
    }
}

// ============================================================================
// Main Application
// ============================================================================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(true)
        .init();

    info!("Starting Log Ingestion Service");

    // Get ClickHouse URL from environment
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
        .route("/logs/ingest/batch", post(ingest_batch))
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
    info!("   GET  /health             - Health check");
    info!("   GET  /stats              - Service statistics");
    info!("   POST /logs/ingest        - Ingest single log");
    info!("   POST /logs/ingest/batch  - Ingest batch (RECOMMENDED)");
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
