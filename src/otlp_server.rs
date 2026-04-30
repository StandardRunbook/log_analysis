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
            let service = attrs.get("service.name").cloned().unwrap_or_default();
            let region = attrs.get("cloud.region").cloned().unwrap_or_default();

            for scope_logs in resource_logs.scope_logs {
                for log_record in scope_logs.log_records {
                    let Some(message) = extract_body_string(log_record.body) else {
                        continue;
                    };

                    // Prefer the record's own time, then the collector's
                    // observed-at time (set per-record by filelog/otlp
                    // receivers — verified empirically), and only fall
                    // back to Utc::now() if both are missing. Without this
                    // fallback chain, a tailed-file burst all gets the
                    // same Utc::now() because it arrives at our handler
                    // within a single millisecond.
                    let timestamp = if log_record.time_unix_nano > 0 {
                        DateTime::<Utc>::from_timestamp_nanos(log_record.time_unix_nano as i64)
                    } else if log_record.observed_time_unix_nano > 0 {
                        DateTime::<Utc>::from_timestamp_nanos(
                            log_record.observed_time_unix_nano as i64,
                        )
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
                        template_id: template_id.map(|tid| tid.to_string()).unwrap_or_default(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry_proto::tonic::common::v1::any_value::Value as AnyVal;
    use opentelemetry_proto::tonic::common::v1::{AnyValue, KeyValue};
    use opentelemetry_proto::tonic::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
    use opentelemetry_proto::tonic::resource::v1::Resource;
    use std::sync::Arc;

    fn kv_string(key: &str, value: &str) -> KeyValue {
        KeyValue {
            key: key.into(),
            value: Some(AnyValue {
                value: Some(AnyVal::StringValue(value.into())),
            }),
        }
    }

    fn kv_int(key: &str, value: i64) -> KeyValue {
        KeyValue {
            key: key.into(),
            value: Some(AnyValue {
                value: Some(AnyVal::IntValue(value)),
            }),
        }
    }

    #[test]
    fn extract_string_attrs_picks_only_string_values() {
        // Strings are kept; non-string types (int, bool, ...) are dropped
        // because the schema only stores string attribute fields.
        let attrs = vec![
            kv_string("org_id", "tenant-1"),
            kv_int("port", 8080),
            kv_string("service.name", "api"),
        ];
        let map = extract_string_attrs(&attrs);
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("org_id").map(String::as_str), Some("tenant-1"));
        assert_eq!(map.get("service.name").map(String::as_str), Some("api"));
        assert!(!map.contains_key("port"));
    }

    #[test]
    fn extract_string_attrs_handles_missing_inner_value() {
        // KeyValue with .value = None should be dropped, not panicked.
        let attrs = vec![KeyValue {
            key: "weird".into(),
            value: None,
        }];
        let map = extract_string_attrs(&attrs);
        assert!(map.is_empty());
    }

    #[test]
    fn extract_body_string_round_trip() {
        let body = Some(AnyValue {
            value: Some(AnyVal::StringValue("hello".into())),
        });
        assert_eq!(extract_body_string(body).as_deref(), Some("hello"));
    }

    #[test]
    fn extract_body_string_none_for_non_string_or_missing() {
        // Missing body
        assert_eq!(extract_body_string(None), None);
        // Wrapper present but value-less
        assert_eq!(extract_body_string(Some(AnyValue { value: None })), None);
        // Non-string body type (int)
        let int_body = Some(AnyValue {
            value: Some(AnyVal::IntValue(42)),
        });
        assert_eq!(extract_body_string(int_body), None);
    }

    // ---------- end-to-end export() with in-memory pipeline ----------

    /// Build an OtlpLogsServer hooked up to a wiremock ClickHouse so we
    /// can drive the gRPC handler without standing up a real backend.
    /// Returns the server, the matcher (so the test can pre-load
    /// templates), and the unmatched-queue receiver (so the test can
    /// observe what was queued for the LLM consumer).
    async fn make_server() -> (OtlpLogsServer, Arc<LogMatcher>, mpsc::Receiver<LogEntry>) {
        // wiremock as a stand-in for ClickHouse — accepts any insert.
        let mock_ch = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .mount(&mock_ch)
            .await;

        let ch = Arc::new(crate::clickhouse_client::ClickHouseClient::new(&mock_ch.uri()).unwrap());
        let writer = Arc::new(crate::buffered_writer::BufferedClickHouseWriter::new(
            ch,
            10,
            std::time::Duration::from_secs(5),
        ));
        let _writer_handle = writer.clone().start_background_flusher();

        let matcher = Arc::new(LogMatcher::new());
        let (tx, rx) = mpsc::channel::<LogEntry>(64);
        (
            OtlpLogsServer::new(matcher.clone(), writer, tx),
            matcher,
            rx,
        )
    }

    fn record_with_body(body: &str, time_ns: u64, observed_ns: u64) -> LogRecord {
        LogRecord {
            time_unix_nano: time_ns,
            observed_time_unix_nano: observed_ns,
            severity_number: 0,
            severity_text: String::new(),
            body: Some(AnyValue {
                value: Some(AnyVal::StringValue(body.into())),
            }),
            attributes: vec![],
            dropped_attributes_count: 0,
            flags: 0,
            trace_id: vec![],
            span_id: vec![],
        }
    }

    fn request_with(records: Vec<LogRecord>, attrs: Vec<KeyValue>) -> ExportLogsServiceRequest {
        ExportLogsServiceRequest {
            resource_logs: vec![ResourceLogs {
                resource: Some(Resource {
                    attributes: attrs,
                    dropped_attributes_count: 0,
                }),
                scope_logs: vec![ScopeLogs {
                    scope: None,
                    log_records: records,
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        }
    }

    #[tokio::test]
    async fn export_routes_matched_to_writer_and_unmatched_to_queue() {
        let (server, matcher, mut unmatched_rx) = make_server().await;
        // Register one template; matched logs will hit it.
        matcher.add_template(crate::log_matcher::LogTemplate {
            template_id: 100,
            pattern: r"hello world".to_string(),
            variables: vec![],
            example: "hello world".into(),
        });

        let req = request_with(
            vec![
                record_with_body("hello world", 1_700_000_000_000_000_000, 0),
                record_with_body("a totally novel log shape", 1_700_000_000_000_000_001, 0),
            ],
            vec![
                kv_string("org_id", "tenant-1"),
                kv_string("log_stream_id", "stream-a"),
                kv_string("service.name", "api"),
                kv_string("cloud.region", "us-east-1"),
            ],
        );
        let resp = server.export(Request::new(req)).await.unwrap();
        assert!(resp.into_inner().partial_success.is_none());

        // The unmatched record must land on the queue.
        let entry = unmatched_rx
            .recv()
            .await
            .expect("unmatched record should be queued");
        assert_eq!(entry.org_id, "tenant-1");
        assert_eq!(entry.log_stream_id, "stream-a");
        assert_eq!(entry.service, "api");
        assert_eq!(entry.region, "us-east-1");
        assert_eq!(entry.message, "a totally novel log shape");
        assert_eq!(entry.template_id, ""); // unmatched → empty template_id

        // No more unmatched records should be pending.
        assert!(unmatched_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn export_skips_records_with_non_string_body() {
        // A LogRecord whose body is an int / missing must be silently
        // skipped (don't bail the whole request).
        let (server, _matcher, mut unmatched_rx) = make_server().await;
        let mut int_record = record_with_body("placeholder", 0, 0);
        int_record.body = Some(AnyValue {
            value: Some(AnyVal::IntValue(42)),
        });
        let req = request_with(vec![int_record], vec![]);
        server.export(Request::new(req)).await.unwrap();
        // Nothing should have been queued.
        assert!(unmatched_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn export_uses_default_org_when_attrs_missing() {
        let (server, _m, mut unmatched_rx) = make_server().await;
        let req = request_with(
            vec![record_with_body("novel shape with no template", 1, 0)],
            vec![], // no org_id, no log_stream_id, no service.name
        );
        server.export(Request::new(req)).await.unwrap();
        let entry = unmatched_rx.recv().await.unwrap();
        assert_eq!(entry.org_id, "default");
        assert_eq!(entry.log_stream_id, "default-stream");
        assert_eq!(entry.log_stream_name, "default-stream");
        assert_eq!(entry.service, "");
        assert_eq!(entry.region, "");
    }

    #[tokio::test]
    async fn export_falls_back_to_observed_time_then_now() {
        // record A: time_unix_nano = 0, observed = some value → observed wins
        // record B: both 0 → falls through to Utc::now() (test just
        //                  verifies the timestamp is recent, not zero)
        let (server, _m, mut unmatched_rx) = make_server().await;
        let req = request_with(
            vec![
                record_with_body("novel A", 0, 1_700_000_000_000_000_000),
                record_with_body("novel B", 0, 0),
            ],
            vec![],
        );
        server.export(Request::new(req)).await.unwrap();
        let a = unmatched_rx.recv().await.unwrap();
        // A's timestamp should match the observed_time we set.
        assert_eq!(
            a.timestamp.timestamp_nanos_opt(),
            Some(1_700_000_000_000_000_000)
        );
        let b = unmatched_rx.recv().await.unwrap();
        // B falls through to Utc::now() — we can't assert exact value
        // but it must be recent (within the last 5 seconds).
        let age = (Utc::now() - b.timestamp).num_seconds().abs();
        assert!(age < 5, "fallback timestamp not recent: {b:?}");
    }

    #[tokio::test]
    async fn export_empty_request_is_ok() {
        let (server, _m, _rx) = make_server().await;
        let req = ExportLogsServiceRequest {
            resource_logs: vec![],
        };
        let resp = server.export(Request::new(req)).await.unwrap();
        assert!(resp.into_inner().partial_success.is_none());
    }

    #[test]
    fn into_service_returns_a_grpc_server() {
        // Smoke test: the conversion compiles and produces a value of the
        // expected wrapper type. We don't actually serve over a port —
        // that would require a tonic transport setup that's overkill for
        // unit tests.
        let server = OtlpLogsServer::new(
            Arc::new(LogMatcher::new()),
            // Build a trivially-constructed writer pointed at a non-listening
            // URL. We never call into it in this test.
            Arc::new(crate::buffered_writer::BufferedClickHouseWriter::new(
                Arc::new(
                    crate::clickhouse_client::ClickHouseClient::new("http://127.0.0.1:1").unwrap(),
                ),
                1,
                std::time::Duration::from_secs(1),
            )),
            mpsc::channel(1).0,
        );
        let _service: LogsServiceServer<OtlpLogsServer> = server.into_service();
    }
}
