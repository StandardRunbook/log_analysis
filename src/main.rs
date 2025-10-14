mod config;
mod histogram;
mod jsd;
mod llm_service;
mod log_matcher;
mod log_stream_client;
mod metadata_service;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::post,
    Json, Router,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;
use tracing_subscriber;

use config::Config;
use histogram::Histogram;
use jsd::{calculate_jsd, get_top_contributors};
use llm_service::LLMServiceClient;
use log_matcher::LogMatcher;
use log_stream_client::{LogEntry, LogStreamClient};
use metadata_service::{MetadataQuery, MetadataServiceClient};

#[derive(Debug, Deserialize)]
struct LogQueryRequest {
    // Grafana context (all required)
    org_id: String,
    dashboard: String,
    panel_title: String,
    metric_name: String,

    // Time range (required)
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct ProcessedLog {
    timestamp: String,
    content: String,
    stream_id: String,
    matched_template: Option<u64>,
    extracted_values: std::collections::HashMap<String, String>,
}

#[derive(Debug, Serialize)]
struct LogGroup {
    representative_logs: Vec<String>,
    relative_change: f64,
}

#[derive(Debug, Serialize)]
struct LogQueryResponse {
    log_groups: Vec<LogGroup>,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

// Application state shared across handlers
struct AppState {
    metadata_client: MetadataServiceClient,
    log_stream_client: LogStreamClient,
    log_matcher: Arc<tokio::sync::RwLock<LogMatcher>>,
    llm_client: LLMServiceClient,
}

/// Middleware to log incoming requests from Grafana
async fn log_request_middleware(req: Request<Body>, next: Next) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = req.headers().clone();

    // Log the incoming request
    info!("═══════════════════════════════════════════════════════");
    info!("📥 INCOMING REQUEST FROM GRAFANA");
    info!("   Method: {}", method);
    info!("   URI: {}", uri);

    // Log relevant headers
    if let Some(user_agent) = headers.get("user-agent") {
        info!(
            "   User-Agent: {}",
            user_agent.to_str().unwrap_or("invalid")
        );
    }
    if let Some(content_type) = headers.get("content-type") {
        info!(
            "   Content-Type: {}",
            content_type.to_str().unwrap_or("invalid")
        );
    }
    if let Some(origin) = headers.get("origin") {
        info!("   Origin: {}", origin.to_str().unwrap_or("invalid"));
    }

    info!("═══════════════════════════════════════════════════════");

    // Process the request
    let response = next.run(req).await;

    // Log the response status
    info!("📤 RESPONSE: Status {}", response.status());
    info!("───────────────────────────────────────────────────────\n");

    response
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Load configuration from environment variables
    let config = match Config::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::error!("❌ Configuration error: {}", e);
            tracing::error!("💡 Please set the required environment variables:");
            tracing::error!(
                "   - METADATA_GRPC_ENDPOINT: gRPC endpoint for metadata service (e.g., http://localhost:50051)"
            );
            tracing::error!(
                "   - CLICKHOUSE_URL: ClickHouse server URL (e.g., http://localhost:8123)"
            );
            tracing::error!("   - CLICKHOUSE_USER: ClickHouse username");
            tracing::error!("   - CLICKHOUSE_PASSWORD: ClickHouse password");
            tracing::error!(
                "   - CLICKHOUSE_DATABASE: ClickHouse database (optional, default: default)"
            );
            tracing::error!("   - LLM_PROVIDER: LLM provider (openai, anthropic, cohere)");
            tracing::error!("   - LLM_API_KEY: API key for LLM service");
            tracing::error!("   - LLM_MODEL: Model name (optional, auto-detected from provider)");
            std::process::exit(1);
        }
    };

    config.log_config();

    // Initialize services with configuration
    let metadata_client = MetadataServiceClient::new(config.metadata_grpc_endpoint.clone());
    let log_stream_client = LogStreamClient::new();
    let log_matcher = Arc::new(tokio::sync::RwLock::new(LogMatcher::new()));
    let llm_client = LLMServiceClient::new(
        config.llm_provider.clone(),
        config.llm_api_key.clone(),
        config.llm_model.clone(),
    );

    let app_state = Arc::new(AppState {
        metadata_client,
        log_stream_client,
        log_matcher,
        llm_client,
    });

    // Configure CORS to allow requests from any origin (including Grafana)
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Build the application router
    let app = Router::new()
        .route("/query_logs", post(query_logs_handler))
        .with_state(app_state)
        .layer(cors)
        .layer(middleware::from_fn(log_request_middleware));

    // Define the address to bind to (hardcoded for REST API)
    let addr = SocketAddr::from(([127, 0, 0, 1], 3001));
    info!("🚀 Log Analyzer API starting on {}", addr);
    info!("📡 Waiting for requests from Grafana plugin...");

    // Start the server
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn query_logs_handler(
    State(state): State<Arc<AppState>>,
    payload: Result<Json<LogQueryRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<LogQueryResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Better error handling for JSON parsing
    let Json(payload) = payload.map_err(|err| {
        tracing::error!("Failed to parse JSON request: {}", err);
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("Invalid JSON request body: {}", err),
            }),
        )
    })?;

    info!("═══════════════════════════════════════════════════════");
    info!("📊 Grafana Query Context:");
    info!("   Org ID: {}", payload.org_id);
    info!("   Dashboard: {}", payload.dashboard);
    info!("   Panel: {}", payload.panel_title);
    info!("   Metric: {}", payload.metric_name);
    info!("   Time: {} to {}", payload.start_time, payload.end_time);
    info!("═══════════════════════════════════════════════════════");

    // Validate time range
    if payload.start_time >= payload.end_time {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "start_time must be before end_time".to_string(),
            }),
        ));
    }

    // Calculate baseline time range (3 hours before start_time)
    let baseline_duration = Duration::hours(3);
    let baseline_end = payload.start_time;
    let baseline_start = baseline_end - baseline_duration;

    info!("Baseline period: {} to {}", baseline_start, baseline_end);

    // Query baseline logs (3 hours prior)
    let baseline_histogram = query_and_build_histogram(
        &state,
        &payload.org_id,
        &payload.dashboard,
        &payload.panel_title,
        &payload.metric_name,
        baseline_start,
        baseline_end,
    )
    .await?;

    info!(
        "Baseline histogram: {} logs, {} unique templates",
        baseline_histogram.total,
        baseline_histogram.counts.len()
    );

    // Query current period logs
    let (current_histogram, processed_logs, _matched_count, _unmatched_count, _new_templates_count) =
        query_and_process_logs(
            &state,
            &payload.org_id,
            &payload.dashboard,
            &payload.panel_title,
            &payload.metric_name,
            payload.start_time,
            payload.end_time,
        )
        .await?;

    info!(
        "Current histogram: {} logs, {} unique templates",
        current_histogram.total,
        current_histogram.counts.len()
    );

    // Calculate JSD if we have baseline data
    if baseline_histogram.total > 0 && current_histogram.total > 0 {
        let jsd_result = calculate_jsd(&baseline_histogram, &current_histogram);

        // Populate representative logs for each template (sorted by contribution already)
        let mut top_contributors = get_top_contributors(&jsd_result, 10);
        for contributor in &mut top_contributors {
            // Get up to 2 representative logs for this template from processed_logs
            let representative = processed_logs
                .iter()
                .filter(|log| log.matched_template.as_ref() == Some(&contributor.template_id))
                .take(2)
                .map(|log| log.content.clone())
                .collect::<Vec<_>>();

            if !representative.is_empty() {
                contributor.representative_logs = Some(representative);
            }
        }

        info!(
            "JSD Score: {:.6}, Top contributor: {}",
            jsd_result.jsd_score,
            top_contributors
                .first()
                .map(|c| c.template_id.to_string())
                .unwrap_or("none".to_string())
        );

        // Convert to simplified response structure
        let log_groups = top_contributors
            .into_iter()
            .filter_map(|contributor| {
                contributor.representative_logs.map(|logs| LogGroup {
                    representative_logs: logs,
                    relative_change: contributor.relative_change,
                })
            })
            .collect();

        Ok(Json(LogQueryResponse { log_groups }))
    } else {
        info!("Insufficient data for JSD calculation");
        Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Insufficient data for JSD calculation".to_string(),
            }),
        ))
    }
}

/// Query logs and build histogram (for baseline calculation)
async fn query_and_build_histogram(
    state: &AppState,
    org_id: &str,
    dashboard: &str,
    graph_name: &str,
    metric_name: &str,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
) -> Result<Histogram, (StatusCode, Json<ErrorResponse>)> {
    let metadata_query = MetadataQuery {
        org_id: org_id.to_string(),
        dashboard: dashboard.to_string(),
        graph_name: graph_name.to_string(),
        metric_name: metric_name.to_string(),
        start_time,
        end_time,
    };

    let log_streams = state
        .metadata_client
        .get_log_streams(&metadata_query)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to query metadata service: {}", e),
                }),
            )
        })?;

    let mut all_logs = Vec::new();
    for stream in log_streams {
        if let Ok(logs) = state
            .log_stream_client
            .download_logs(&stream, start_time, end_time)
            .await
        {
            all_logs.extend(logs);
        }
    }

    let histogram = build_histogram_from_logs(state, &all_logs).await;
    Ok(histogram)
}

/// Query logs, process them, and build histogram (for current period)
async fn query_and_process_logs(
    state: &AppState,
    org_id: &str,
    dashboard: &str,
    graph_name: &str,
    metric_name: &str,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
) -> Result<(Histogram, Vec<ProcessedLog>, usize, usize, usize), (StatusCode, Json<ErrorResponse>)>
{
    let metadata_query = MetadataQuery {
        org_id: org_id.to_string(),
        dashboard: dashboard.to_string(),
        graph_name: graph_name.to_string(),
        metric_name: metric_name.to_string(),
        start_time,
        end_time,
    };

    let log_streams = state
        .metadata_client
        .get_log_streams(&metadata_query)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to query metadata service: {}", e),
                }),
            )
        })?;

    info!("Found {} log streams to query", log_streams.len());

    let mut all_logs = Vec::new();
    for stream in log_streams {
        match state
            .log_stream_client
            .download_logs(&stream, start_time, end_time)
            .await
        {
            Ok(logs) => {
                info!(
                    "Downloaded {} logs from stream {}",
                    logs.len(),
                    stream.stream_id
                );
                all_logs.extend(logs);
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to download logs from stream {}: {}",
                    stream.stream_id,
                    e
                );
            }
        }
    }

    info!("Total logs downloaded: {}", all_logs.len());

    let mut histogram = Histogram::new();
    let mut processed_logs = Vec::new();
    let mut matched_count = 0;
    let mut unmatched_count = 0;
    let mut new_templates_count = 0;

    for log_entry in all_logs {
        let match_result = {
            let matcher = state.log_matcher.read().await;
            matcher.match_log(&log_entry.content)
        };

        let (template_id, extracted_values) = if match_result.matched {
            matched_count += 1;
            (
                match_result.template_id.clone(),
                match_result.extracted_values,
            )
        } else {
            unmatched_count += 1;
            info!("No template match for log: {}", log_entry.content);

            match state.llm_client.generate_template(&log_entry.content).await {
                Ok(new_template) => {
                    let template_id = new_template.template_id.clone();
                    info!("Generated new template: {}", template_id);

                    {
                        let mut matcher = state.log_matcher.write().await;
                        matcher.add_template(new_template);
                    }

                    new_templates_count += 1;

                    let new_match = {
                        let matcher = state.log_matcher.read().await;
                        matcher.match_log(&log_entry.content)
                    };

                    (new_match.template_id.clone(), new_match.extracted_values)
                }
                Err(e) => {
                    tracing::warn!("Failed to generate template: {}", e);
                    (None, std::collections::HashMap::new())
                }
            }
        };

        // Add to histogram if we have a template
        if let Some(tid) = template_id {
            histogram.add(tid);
        }

        processed_logs.push(ProcessedLog {
            timestamp: log_entry.timestamp.to_rfc3339(),
            content: log_entry.content,
            stream_id: log_entry.stream_id,
            matched_template: template_id,
            extracted_values,
        });
    }

    info!(
        "Processing complete: {} matched, {} unmatched, {} new templates",
        matched_count, unmatched_count, new_templates_count
    );

    Ok((
        histogram,
        processed_logs,
        matched_count,
        unmatched_count,
        new_templates_count,
    ))
}

/// Build histogram from log entries (matching only, no LLM generation)
async fn build_histogram_from_logs(state: &AppState, logs: &[LogEntry]) -> Histogram {
    let mut histogram = Histogram::new();

    for log_entry in logs {
        let match_result = {
            let matcher = state.log_matcher.read().await;
            matcher.match_log(&log_entry.content)
        };

        if match_result.matched {
            if let Some(template_id) = match_result.template_id {
                histogram.add(template_id);
            }
        }
    }

    histogram
}
