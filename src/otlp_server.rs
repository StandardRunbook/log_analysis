//! OTLP/gRPC ingest server.
//!
//! Implements the OpenTelemetry `LogsService` so customer collectors can
//! export log records to us via the standard OTLP/gRPC protocol. Same
//! matcher / writer / LLM-queue state as the JSON HTTP handler, just a
//! different framing on the wire.
//!
//! Resource attributes used (per OTel semantic conventions plus a couple
//! of our own):
//!   - `org_id` (custom)        → tenant identifier
//!   - `log_stream_id` (custom) → fine-grained stream within a tenant
//!   - `log_stream_name` (cust) → human-readable stream name
//!   - `service.name`           → service field
//!   - `cloud.region`           → region field
//!
//! Anything missing is filled with sensible defaults; an authn layer
//! upstream is the right place to enforce `org_id` presence.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use opentelemetry_proto::tonic::collector::logs::v1::{
    logs_service_server::{LogsService, LogsServiceServer},
    ExportLogsServiceRequest, ExportLogsServiceResponse,
};
use opentelemetry_proto::tonic::common::v1::{any_value::Value as AnyVal, AnyValue, KeyValue};
use tokio::sync::mpsc;
use tonic::{Request, Response, Status};
use tracing::debug;

use crate::buffered_writer::BufferedClickHouseWriter;
use crate::clickhouse_client::LogEntry;
use crate::log_matcher::LogMatcher;

pub struct OtlpLogsServer {
    matcher: Arc<LogMatcher>,
    writer: Arc<BufferedClickHouseWriter>,
    unmatched_tx: mpsc::Sender<LogEntry>,
}

impl OtlpLogsServer {
    pub fn new(
        matcher: Arc<LogMatcher>,
        writer: Arc<BufferedClickHouseWriter>,
        unmatched_tx: mpsc::Sender<LogEntry>,
    ) -> Self {
        Self {
            matcher,
            writer,
            unmatched_tx,
        }
    }

    pub fn into_service(self) -> LogsServiceServer<Self> {
        LogsServiceServer::new(self)
    }
}

#[tonic::async_trait]
impl LogsService for OtlpLogsServer {
    async fn export(
        &self,
        request: Request<ExportLogsServiceRequest>,
    ) -> Result<Response<ExportLogsServiceResponse>, Status> {
        let req = request.into_inner();

        // Accumulate matched entries across the whole request and send
        // them to the writer in one channel op at the end.
        let mut matched_entries: Vec<LogEntry> = Vec::new();

        for resource_logs in req.resource_logs {
            let attrs = resource_logs
                .resource
                .as_ref()
                .map(|r| extract_string_attrs(&r.attributes))
                .unwrap_or_default();

            // Resource-level fields apply to every record under this
            // ResourceLogs. Pull them out once instead of per-record.
            let org_id = attrs
                .get("org_id")
                .cloned()
                .unwrap_or_else(|| "default".to_string());
            let log_stream_id = attrs
                .get("log_stream_id")
                .cloned()
                .unwrap_or_else(|| "default-stream".to_string());
            let log_stream_name = attrs
                .get("log_stream_name")
                .cloned()
                .unwrap_or_else(|| log_stream_id.clone());
            let service = attrs
                .get("service.name")
                .cloned()
                .unwrap_or_default();
            let region = attrs
                .get("cloud.region")
                .cloned()
                .unwrap_or_default();

            for scope_logs in resource_logs.scope_logs {
                for log_record in scope_logs.log_records {
                    let Some(message) = extract_body_string(log_record.body) else {
                        continue;
                    };

                    let timestamp = if log_record.time_unix_nano > 0 {
                        DateTime::<Utc>::from_timestamp_nanos(log_record.time_unix_nano as i64)
                    } else {
                        Utc::now()
                    };

                    let template_id = self.matcher.match_log(&message);

                    let entry = LogEntry {
                        org_id: org_id.clone(),
                        log_stream_id: log_stream_id.clone(),
                        service: service.clone(),
                        region: region.clone(),
                        log_stream_name: log_stream_name.clone(),
                        timestamp,
                        template_id: template_id
                            .map(|tid| tid.to_string())
                            .unwrap_or_default(),
                        message,
                    };

                    match template_id {
                        Some(_) => matched_entries.push(entry),
                        None => {
                            if let Err(e) = self.unmatched_tx.try_send(entry) {
                                debug!("OTLP unmatched queue overflow: {}", e);
                            }
                        }
                    }
                }
            }
        }

        if !matched_entries.is_empty() {
            self.writer.write_batch(matched_entries).await;
        }

        Ok(Response::new(ExportLogsServiceResponse {
            partial_success: None,
        }))
    }
}

fn extract_string_attrs(kvs: &[KeyValue]) -> HashMap<String, String> {
    kvs.iter()
        .filter_map(|kv| {
            kv.value
                .as_ref()
                .and_then(|v| v.value.as_ref())
                .and_then(|v| match v {
                    AnyVal::StringValue(s) => Some((kv.key.clone(), s.clone())),
                    _ => None,
                })
        })
        .collect()
}

fn extract_body_string(body: Option<AnyValue>) -> Option<String> {
    body.and_then(|av| av.value).and_then(|v| match v {
        AnyVal::StringValue(s) => Some(s),
        _ => None,
    })
}
