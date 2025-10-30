/// ClickHouse client for log storage
///
/// Provides high-performance log ingestion and querying using ClickHouse

use anyhow::Result;
use clickhouse::Client;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Deserialize, clickhouse::Row)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub org: String,
    pub dashboard: String,
    pub panel_name: String,
    pub metric_name: String,
    pub service: String,
    pub host: String,
    pub level: String,
    pub message: String,
    pub template_id: Option<u64>,
    pub template_pattern: Option<String>,
    pub metadata: String,
}

// Custom serialization for ClickHouse JSON format
impl Serialize for LogEntry {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("LogEntry", 12)?;
        // Format timestamp as "YYYY-MM-DD HH:MM:SS" for ClickHouse
        let ts_str = self.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();
        state.serialize_field("timestamp", &ts_str)?;
        state.serialize_field("org", &self.org)?;
        state.serialize_field("dashboard", &self.dashboard)?;
        state.serialize_field("panel_name", &self.panel_name)?;
        state.serialize_field("metric_name", &self.metric_name)?;
        state.serialize_field("service", &self.service)?;
        state.serialize_field("host", &self.host)?;
        state.serialize_field("level", &self.level)?;
        state.serialize_field("message", &self.message)?;
        state.serialize_field("template_id", &self.template_id)?;
        state.serialize_field("template_pattern", &self.template_pattern)?;
        state.serialize_field("metadata", &self.metadata)?;
        state.end()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, clickhouse::Row)]
pub struct TemplateRow {
    pub template_id: u64,
    pub pattern: String,
    pub variables: Vec<String>,
    pub example: String,
}

#[derive(Clone)]
pub struct ClickHouseClient {
    client: Client,
    url: String,
}

impl ClickHouseClient {
    /// Create a new ClickHouse client
    pub fn new(url: &str) -> Result<Self> {
        let mut client = Client::default()
            .with_url(url)
            .with_database("default");

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

        Ok(Self { client, url: url.to_string() })
    }

    /// Initialize database schema
    pub async fn init_schema(&self) -> Result<()> {
        let schema = include_str!("../clickhouse_schema.sql");

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
        let mut insert = self.client.insert("logs")?;
        insert.write(&log).await?;
        insert.end().await?;
        Ok(())
    }

    /// Insert logs in batch (much faster)
    pub async fn insert_logs_batch(&self, logs: Vec<LogEntry>) -> Result<()> {
        if logs.is_empty() {
            return Ok(());
        }

        // Use HTTP JSON format instead of binary Row format (more reliable)
        let json_lines: Vec<String> = logs.iter()
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
        org: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<LogEntry>> {
        let logs = self.client
            .query("
                SELECT
                    timestamp, org, dashboard, service, host, level,
                    message, template_id, template_pattern, metadata
                FROM logs
                WHERE org = ?
                  AND timestamp >= ?
                  AND timestamp <= ?
                ORDER BY timestamp DESC
                LIMIT 10000
            ")
            .bind(org)
            .bind(start_time)
            .bind(end_time)
            .fetch_all::<LogEntry>()
            .await?;

        Ok(logs)
    }

    /// Query logs grouped by template
    pub async fn query_logs_grouped(
        &self,
        org: &str,
        dashboard: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<LogGroup>> {
        #[derive(Debug, clickhouse::Row, Deserialize)]
        struct GroupRow {
            template_id: Option<u64>,
            log_count: u64,
            sample_messages: Vec<String>,
        }

        let groups = self.client
            .query("
                SELECT
                    template_id,
                    count() as log_count,
                    groupArray(5)(message) as sample_messages
                FROM logs
                WHERE org = ?
                  AND dashboard = ?
                  AND timestamp >= ?
                  AND timestamp <= ?
                GROUP BY template_id
                ORDER BY log_count DESC
                LIMIT 20
            ")
            .bind(org)
            .bind(dashboard)
            .bind(start_time)
            .bind(end_time)
            .fetch_all::<GroupRow>()
            .await?;

        Ok(groups.into_iter().map(|g| LogGroup {
            template_id: g.template_id,
            log_count: g.log_count,
            sample_messages: g.sample_messages,
            relative_change: 0.0, // TODO: Calculate from baseline
        }).collect())
    }

    /// Store template
    pub async fn insert_template(&self, template: TemplateRow) -> Result<()> {
        let mut insert = self.client.insert("templates")?;
        insert.write(&template).await?;
        insert.end().await?;
        Ok(())
    }

    /// Get all templates
    pub async fn get_templates(&self) -> Result<Vec<TemplateRow>> {
        let templates = self.client
            .query("SELECT template_id, pattern, variables, example FROM templates")
            .fetch_all::<TemplateRow>()
            .await?;

        Ok(templates)
    }

    /// Insert a template example
    pub async fn insert_template_example(&self, log: &LogEntry) -> Result<()> {
        if log.template_id.is_none() {
            return Ok(()); // Skip logs without templates
        }

        let json_line = serde_json::to_string(log)?;

        let http_client = reqwest::Client::new();
        let response = http_client
            .post(&self.url)
            .query(&[("query", "INSERT INTO template_examples FORMAT JSONEachRow")])
            .body(json_line)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("ClickHouse insert failed: {}", error_text);
        }

        Ok(())
    }

    /// Get example logs for a template
    pub async fn get_template_examples(
        &self,
        template_id: u64,
        org: &str,
        dashboard: &str,
        panel_name: &str,
        metric_name: &str,
        limit: usize,
    ) -> Result<Vec<LogEntry>> {
        let query = format!(
            "SELECT timestamp, org, dashboard, panel_name, metric_name, service, host, level, message, {}, template_pattern, metadata
             FROM template_examples
             WHERE template_id = {}
               AND org = '{}'
               AND dashboard = '{}'
               AND panel_name = '{}'
               AND metric_name = '{}'
             ORDER BY timestamp DESC
             LIMIT {}
             FORMAT JSONEachRow",
            template_id, template_id, org, dashboard, panel_name, metric_name, limit
        );

        let http_client = reqwest::Client::new();
        let response = http_client
            .post(&self.url)
            .body(query)
            .send()
            .await?;

        let body = response.text().await?;
        let examples: Vec<LogEntry> = body
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();

        Ok(examples)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LogGroup {
    pub template_id: Option<u64>,
    pub log_count: u64,
    pub sample_messages: Vec<String>,
    pub relative_change: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let log = LogEntry {
            timestamp: Utc::now(),
            org: "1".to_string(),
            dashboard: "test".to_string(),
            service: "api".to_string(),
            host: "server-01".to_string(),
            level: "ERROR".to_string(),
            message: "Test error".to_string(),
            template_id: Some(1),
            template_pattern: Some("Test.*".to_string()),
            metadata: "{}".to_string(),
        };

        client.insert_log(log.clone()).await.unwrap();

        let logs = client
            .query_logs("1", Utc::now() - chrono::Duration::hours(1), Utc::now())
            .await
            .unwrap();

        assert!(!logs.is_empty());
    }
}
