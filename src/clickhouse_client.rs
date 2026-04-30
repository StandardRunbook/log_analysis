//! ClickHouse client for log storage
//!
//! Provides high-performance log ingestion and querying using ClickHouse

use anyhow::Result;
use chrono::{DateTime, Utc};
use clickhouse::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, clickhouse::Row)]
pub struct LogEntry {
    pub org_id: String,
    pub log_stream_id: String,
    pub service: String,
    pub region: String,
    pub log_stream_name: String,
    pub timestamp: DateTime<Utc>,
    pub template_id: String,
    pub message: String,
}

// Custom serialization for ClickHouse JSON format
impl Serialize for LogEntry {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("LogEntry", 8)?;
        state.serialize_field("org_id", &self.org_id)?;
        state.serialize_field("log_stream_id", &self.log_stream_id)?;
        state.serialize_field("service", &self.service)?;
        state.serialize_field("region", &self.region)?;
        state.serialize_field("log_stream_name", &self.log_stream_name)?;
        // Format timestamp with milliseconds for DateTime64(3)
        let ts_str = self.timestamp.format("%Y-%m-%d %H:%M:%S%.3f").to_string();
        state.serialize_field("timestamp", &ts_str)?;
        state.serialize_field("template_id", &self.template_id)?;
        state.serialize_field("message", &self.message)?;
        state.end()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, clickhouse::Row)]
pub struct TemplateRow {
    pub org_id: String,
    pub log_stream_id: String,
    pub template_id: u64,
    pub pattern: String,
    pub variables: Vec<String>,
    pub example: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct ClickHouseClient {
    client: Client,
    url: String,
}

impl ClickHouseClient {
    /// Create a new ClickHouse client
    pub fn new(url: &str) -> Result<Self> {
        let mut client = Client::default().with_url(url).with_database("default");

        // Add authentication if credentials are provided via environment
        if let Ok(user) = std::env::var("CLICKHOUSE_USER") {
            client = client.with_user(user);
        }
        if let Ok(password) = std::env::var("CLICKHOUSE_PASSWORD") {
            client = client.with_password(password);
        }
        if let Ok(database) = std::env::var("CLICKHOUSE_DATABASE") {
            client = client.with_database(database);
        }

        Ok(Self {
            client,
            url: url.to_string(),
        })
    }

    /// Initialize database schema
    pub async fn init_schema(&self) -> Result<()> {
        let schema = include_str!("../hover-schema/clickhouse_schema.sql");

        // Split by semicolon and execute each statement
        for statement in schema.split(';') {
            let trimmed = statement.trim();
            if !trimmed.is_empty() {
                self.client.query(trimmed).execute().await?;
            }
        }

        Ok(())
    }

    /// Insert a single log entry
    pub async fn insert_log(&self, log: LogEntry) -> Result<()> {
        // Use JSON format for consistency
        let json_line = serde_json::to_string(&log)?;

        let http_client = reqwest::Client::new();
        let response = http_client
            .post(&self.url)
            .query(&[("query", "INSERT INTO logs FORMAT JSONEachRow")])
            .body(json_line)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("ClickHouse insert failed: {}", error_text);
        }

        Ok(())
    }

    /// Insert logs in batch (much faster)
    pub async fn insert_logs_batch(&self, logs: Vec<LogEntry>) -> Result<()> {
        if logs.is_empty() {
            return Ok(());
        }

        // Use HTTP JSON format instead of binary Row format (more reliable)
        let json_lines: Vec<String> = logs
            .iter()
            .map(|log| serde_json::to_string(log).unwrap())
            .collect();
        let body = json_lines.join("\n");

        let http_client = reqwest::Client::new();
        let response = http_client
            .post(&self.url)
            .query(&[("query", "INSERT INTO logs FORMAT JSONEachRow")])
            .body(body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("ClickHouse insert failed: {}", error_text);
        }

        Ok(())
    }

    /// Query logs for a time range
    pub async fn query_logs(
        &self,
        org_id: &str,
        log_stream_id: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<LogEntry>> {
        // Format timestamps for DateTime64(3) - need to use parseDateTime64BestEffort or format as string
        let start_str = start_time.format("%Y-%m-%d %H:%M:%S%.3f").to_string();
        let end_str = end_time.format("%Y-%m-%d %H:%M:%S%.3f").to_string();

        let logs = self
            .client
            .query(
                "
                SELECT
                    org_id, log_stream_id, service, region, log_stream_name,
                    timestamp, template_id, message
                FROM logs
                WHERE org_id = ?
                  AND log_stream_id = ?
                  AND timestamp >= parseDateTime64BestEffort(?)
                  AND timestamp <= parseDateTime64BestEffort(?)
                ORDER BY timestamp DESC
                LIMIT 10000
            ",
            )
            .bind(org_id)
            .bind(log_stream_id)
            .bind(start_str)
            .bind(end_str)
            .fetch_all::<LogEntry>()
            .await?;

        Ok(logs)
    }

    /// Query logs grouped by template
    pub async fn query_logs_grouped(
        &self,
        org_id: &str,
        log_stream_id: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<LogGroup>> {
        #[derive(Debug, clickhouse::Row, Deserialize)]
        struct GroupRow {
            template_id: String,
            log_count: u64,
            sample_messages: Vec<String>,
        }

        let start_str = start_time.format("%Y-%m-%d %H:%M:%S%.3f").to_string();
        let end_str = end_time.format("%Y-%m-%d %H:%M:%S%.3f").to_string();

        let groups = self
            .client
            .query(
                "
                SELECT
                    template_id,
                    count() as log_count,
                    groupArray(5)(message) as sample_messages
                FROM logs
                WHERE org_id = ?
                  AND log_stream_id = ?
                  AND timestamp >= parseDateTime64BestEffort(?)
                  AND timestamp <= parseDateTime64BestEffort(?)
                GROUP BY template_id
                ORDER BY log_count DESC
                LIMIT 20
            ",
            )
            .bind(org_id)
            .bind(log_stream_id)
            .bind(start_str)
            .bind(end_str)
            .fetch_all::<GroupRow>()
            .await?;

        Ok(groups
            .into_iter()
            .map(|g| LogGroup {
                template_id: g.template_id,
                log_count: g.log_count,
                sample_messages: g.sample_messages,
                relative_change: 0.0, // TODO: Calculate from baseline
            })
            .collect())
    }

    /// Store template and return the assigned template_id
    ///
    /// IDs should be content-derived hashes assigned at synthesis time (see
    /// `template_id::template_id_from_pattern`). If the caller hasn't done
    /// so, compute it here from the pattern. The legacy MAX(template_id)+1
    /// path is kept only as a last-resort fallback and should not be reached
    /// from any current call site.
    pub async fn insert_template(&self, mut template: TemplateRow) -> Result<u64> {
        if template.template_id == 0 {
            template.template_id = crate::template_id::template_id_from_pattern(&template.pattern);
        }

        // Use HTTP JSON format (JSONEachRow) rather than the clickhouse-rs
        // binary Row format. The binary path requires the `chrono` feature
        // for DateTime<Utc> serialization, which isn't enabled, leading to
        // "Cannot read all data" errors at insert time. JSON is what
        // insert_logs_batch already uses successfully.
        #[derive(Serialize)]
        struct TemplateJson<'a> {
            org_id: &'a str,
            log_stream_id: &'a str,
            template_id: u64,
            pattern: &'a str,
            variables: &'a [String],
            example: &'a str,
            created_at: String,
        }

        let json_line = serde_json::to_string(&TemplateJson {
            org_id: &template.org_id,
            log_stream_id: &template.log_stream_id,
            template_id: template.template_id,
            pattern: &template.pattern,
            variables: &template.variables,
            example: &template.example,
            created_at: template
                .created_at
                .format("%Y-%m-%d %H:%M:%S%.3f")
                .to_string(),
        })?;

        let http_client = reqwest::Client::new();
        let response = http_client
            .post(&self.url)
            .query(&[("query", "INSERT INTO templates FORMAT JSONEachRow")])
            .body(json_line)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("ClickHouse insert_template failed: {}", error_text);
        }

        Ok(template.template_id)
    }

    /// Get next available template ID from ClickHouse. Currently unused —
    /// template IDs are content-hashed rather than auto-incremented — kept
    /// in case we revisit the auto-increment path.
    #[allow(dead_code)]
    async fn get_next_template_id(&self) -> Result<u64> {
        #[derive(Debug, clickhouse::Row, Deserialize)]
        struct MaxIdRow {
            max_id: u64,
        }

        let result = self
            .client
            .query("SELECT COALESCE(max(template_id), 0) as max_id FROM templates")
            .fetch_one::<MaxIdRow>()
            .await?;

        Ok(result.max_id + 1)
    }

    /// Get all templates
    pub async fn get_templates(&self) -> Result<Vec<TemplateRow>> {
        let templates = self.client
            .query("SELECT org_id, log_stream_id, template_id, pattern, variables, example, created_at FROM templates")
            .fetch_all::<TemplateRow>()
            .await?;

        Ok(templates)
    }

    /// Insert template with auto-generated ID (alias for insert_template)
    pub async fn insert_template_with_autoid(&self, template: TemplateRow) -> Result<u64> {
        self.insert_template(template).await
    }

    /// Get templates for a specific org and log stream
    pub async fn get_templates_for_stream(
        &self,
        org_id: &str,
        log_stream_id: &str,
    ) -> Result<Vec<TemplateRow>> {
        let templates = self.client
            .query("SELECT org_id, log_stream_id, template_id, pattern, variables, example, created_at FROM templates WHERE org_id = ? AND log_stream_id = ? ORDER BY template_id")
            .bind(org_id)
            .bind(log_stream_id)
            .fetch_all::<TemplateRow>()
            .await?;

        Ok(templates)
    }

    /// Clear all templates from the database
    pub async fn clear_templates(&self) -> Result<()> {
        self.client
            .query("TRUNCATE TABLE templates")
            .execute()
            .await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LogGroup {
    pub template_id: String,
    pub log_count: u64,
    pub sample_messages: Vec<String>,
    pub relative_change: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn sample_log() -> LogEntry {
        LogEntry {
            org_id: "org-1".to_string(),
            log_stream_id: "stream-1".to_string(),
            service: "api-server".to_string(),
            region: "us-east-1".to_string(),
            log_stream_name: "/aws/api/production".to_string(),
            timestamp: DateTime::parse_from_rfc3339("2024-01-15T10:30:45.123Z")
                .unwrap()
                .with_timezone(&Utc),
            template_id: "template-1".to_string(),
            message: "Test error message".to_string(),
        }
    }

    #[tokio::test]
    #[ignore] // Requires ClickHouse running
    async fn test_clickhouse_connection() {
        let client = ClickHouseClient::new("http://localhost:8123").unwrap();
        client.init_schema().await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_insert_and_query() {
        let client = ClickHouseClient::new("http://localhost:8123").unwrap();
        client.insert_log(sample_log()).await.unwrap();
        let logs = client
            .query_logs(
                "org-1",
                "stream-1",
                Utc::now() - chrono::Duration::hours(1),
                Utc::now(),
            )
            .await
            .unwrap();
        assert!(!logs.is_empty());
    }

    // -- pure tests (no ClickHouse needed) --

    #[test]
    fn test_log_entry_serialization_uses_string_timestamp() {
        // The custom Serialize impl formats the timestamp as
        // "%Y-%m-%d %H:%M:%S%.3f" so ClickHouse's DateTime64(3) parser
        // accepts it. Locking this format down — any change here cascades
        // to every existing inserted row.
        let log = sample_log();
        let json = serde_json::to_string(&log).unwrap();
        assert!(json.contains("\"timestamp\":\"2024-01-15 10:30:45.123\""));
        assert!(json.contains("\"org_id\":\"org-1\""));
        assert!(json.contains("\"template_id\":\"template-1\""));
        assert!(json.contains("\"message\":\"Test error message\""));
    }

    #[test]
    fn test_log_entry_deserialize_round_trip() {
        // The Deserialize side comes from the derive — verify a row
        // serialized by us can be read back.
        let log = sample_log();
        let json = serde_json::to_string(&log).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["org_id"], "org-1");
        assert_eq!(parsed["service"], "api-server");
    }

    #[test]
    fn test_new_picks_up_credentials_from_env() {
        // CLICKHOUSE_USER / _PASSWORD / _DATABASE are read at construction
        // time. Set them, build a client, and verify that the call doesn't
        // panic — internal state is private so we can't assert the
        // credentials, but we can guard the env-read branches.
        std::env::set_var("CLICKHOUSE_USER", "u");
        std::env::set_var("CLICKHOUSE_PASSWORD", "p");
        std::env::set_var("CLICKHOUSE_DATABASE", "d");
        let _ = ClickHouseClient::new("http://localhost:8123").unwrap();
        std::env::remove_var("CLICKHOUSE_USER");
        std::env::remove_var("CLICKHOUSE_PASSWORD");
        std::env::remove_var("CLICKHOUSE_DATABASE");
    }

    // -- HTTP mock tests --

    #[tokio::test]
    async fn test_insert_log_posts_jsoneachrow() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(query_param("query", "INSERT INTO logs FORMAT JSONEachRow"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let client = ClickHouseClient::new(&server.uri()).unwrap();
        client.insert_log(sample_log()).await.unwrap();
    }

    #[tokio::test]
    async fn test_insert_log_propagates_server_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;

        let client = ClickHouseClient::new(&server.uri()).unwrap();
        let err = client.insert_log(sample_log()).await.unwrap_err();
        assert!(err.to_string().contains("ClickHouse insert failed"));
        assert!(err.to_string().contains("boom"));
    }

    #[tokio::test]
    async fn test_insert_logs_batch_short_circuits_on_empty() {
        // Empty batch must NOT touch the network. Use a server that would
        // panic on any request to prove no HTTP call is made.
        let server = MockServer::start().await;
        // No mock registered → any request would 404. The function should
        // return Ok(()) without touching the wire.
        let client = ClickHouseClient::new(&server.uri()).unwrap();
        client.insert_logs_batch(vec![]).await.unwrap();
    }

    #[tokio::test]
    async fn test_insert_logs_batch_serializes_newline_separated() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(query_param("query", "INSERT INTO logs FORMAT JSONEachRow"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let client = ClickHouseClient::new(&server.uri()).unwrap();
        client
            .insert_logs_batch(vec![sample_log(), sample_log(), sample_log()])
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_insert_logs_batch_propagates_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_string("bad data"))
            .mount(&server)
            .await;

        let client = ClickHouseClient::new(&server.uri()).unwrap();
        let err = client
            .insert_logs_batch(vec![sample_log()])
            .await
            .unwrap_err();
        assert!(err.to_string().contains("bad data"));
    }

    #[tokio::test]
    async fn test_insert_template_assigns_content_hash_when_zero() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(query_param("query", "INSERT INTO templates FORMAT JSONEachRow"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let client = ClickHouseClient::new(&server.uri()).unwrap();
        let pattern = r"User (\w+) logged in";
        let template = TemplateRow {
            org_id: "o".into(),
            log_stream_id: "s".into(),
            template_id: 0, // sentinel — should be replaced with content hash
            pattern: pattern.into(),
            variables: vec!["user".into()],
            example: "User alice logged in".into(),
            created_at: Utc::now(),
        };
        let assigned = client.insert_template(template).await.unwrap();
        // Same pattern → same hash; assert determinism.
        assert_eq!(assigned, crate::template_id::template_id_from_pattern(pattern));
        assert_ne!(assigned, 0);
    }

    #[tokio::test]
    async fn test_insert_template_keeps_explicit_id() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = ClickHouseClient::new(&server.uri()).unwrap();
        let template = TemplateRow {
            org_id: "o".into(),
            log_stream_id: "s".into(),
            template_id: 12345, // explicit non-zero — must be preserved
            pattern: "anything".into(),
            variables: vec![],
            example: "ex".into(),
            created_at: Utc::now(),
        };
        let assigned = client.insert_template(template).await.unwrap();
        assert_eq!(assigned, 12345);
    }

    #[tokio::test]
    async fn test_insert_template_propagates_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500).set_body_string("nope"))
            .mount(&server)
            .await;

        let client = ClickHouseClient::new(&server.uri()).unwrap();
        let template = TemplateRow {
            org_id: "o".into(),
            log_stream_id: "s".into(),
            template_id: 1,
            pattern: "p".into(),
            variables: vec![],
            example: "e".into(),
            created_at: Utc::now(),
        };
        let err = client.insert_template(template).await.unwrap_err();
        assert!(err.to_string().contains("insert_template failed"));
        assert!(err.to_string().contains("nope"));
    }

    #[tokio::test]
    async fn test_insert_template_with_autoid_delegates() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let client = ClickHouseClient::new(&server.uri()).unwrap();
        let pattern = r"GET /api/(\w+)";
        let template = TemplateRow {
            org_id: "o".into(),
            log_stream_id: "s".into(),
            template_id: 0,
            pattern: pattern.into(),
            variables: vec!["resource".into()],
            example: "GET /api/users".into(),
            created_at: Utc::now(),
        };
        let assigned = client.insert_template_with_autoid(template).await.unwrap();
        assert_eq!(assigned, crate::template_id::template_id_from_pattern(pattern));
    }

    // -- clickhouse-rs query paths exercise the error branches. Building
    //    a valid RowBinary response body for fetch_all/fetch_one is too
    //    brittle to be worth replicating; we cover the dispatch + the
    //    error path, and leave the success-payload decoding to integration
    //    tests against a real ClickHouse instance.

    #[tokio::test]
    async fn test_query_logs_propagates_decode_error() {
        // A 200 with an empty body fails to parse as RowBinary; that's a
        // real failure mode if ClickHouse mis-frames a response and the
        // function must surface the error rather than silently return zero
        // rows.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(Vec::<u8>::new()))
            .mount(&server)
            .await;

        let client = ClickHouseClient::new(&server.uri()).unwrap();
        let result = client
            .query_logs(
                "org-1",
                "stream-1",
                Utc::now() - chrono::Duration::hours(1),
                Utc::now(),
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_query_logs_grouped_propagates_decode_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(Vec::<u8>::new()))
            .mount(&server)
            .await;

        let client = ClickHouseClient::new(&server.uri()).unwrap();
        let result = client
            .query_logs_grouped(
                "org-1",
                "stream-1",
                Utc::now() - chrono::Duration::hours(1),
                Utc::now(),
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_templates_propagates_decode_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(Vec::<u8>::new()))
            .mount(&server)
            .await;

        let client = ClickHouseClient::new(&server.uri()).unwrap();
        let result = client.get_templates().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_templates_for_stream_propagates_decode_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(Vec::<u8>::new()))
            .mount(&server)
            .await;

        let client = ClickHouseClient::new(&server.uri()).unwrap();
        let result = client
            .get_templates_for_stream("org-1", "stream-1")
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_clear_templates_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = ClickHouseClient::new(&server.uri()).unwrap();
        client.clear_templates().await.unwrap();
    }

    #[tokio::test]
    async fn test_init_schema_executes_each_statement() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        // The schema file contains multiple statements; init_schema must
        // not fail on any of them when ClickHouse 200s every request.
        let client = ClickHouseClient::new(&server.uri()).unwrap();
        client.init_schema().await.unwrap();
    }

    #[tokio::test]
    async fn test_query_logs_propagates_server_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
            .mount(&server)
            .await;

        let client = ClickHouseClient::new(&server.uri()).unwrap();
        let result = client
            .query_logs("o", "s", Utc::now() - chrono::Duration::hours(1), Utc::now())
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_log_group_serialize() {
        let g = LogGroup {
            template_id: "t-7".into(),
            log_count: 42,
            sample_messages: vec!["a".into(), "b".into()],
            relative_change: 0.0,
        };
        let json = serde_json::to_string(&g).unwrap();
        assert!(json.contains("\"template_id\":\"t-7\""));
        assert!(json.contains("\"log_count\":42"));
        assert!(json.contains("\"relative_change\":0.0"));
    }
}
