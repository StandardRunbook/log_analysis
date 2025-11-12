/// ClickHouse client for log storage
///
/// Provides high-performance log ingestion and querying using ClickHouse

use anyhow::Result;
use clickhouse::Client;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

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
        org_id: &str,
        log_stream_id: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<LogEntry>> {
        // Format timestamps for DateTime64(3) - need to use parseDateTime64BestEffort or format as string
        let start_str = start_time.format("%Y-%m-%d %H:%M:%S%.3f").to_string();
        let end_str = end_time.format("%Y-%m-%d %H:%M:%S%.3f").to_string();

        let logs = self.client
            .query("
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
            ")
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

        let groups = self.client
            .query("
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
            ")
            .bind(org_id)
            .bind(log_stream_id)
            .bind(start_str)
            .bind(end_str)
            .fetch_all::<GroupRow>()
            .await?;

        Ok(groups.into_iter().map(|g| LogGroup {
            template_id: g.template_id,
            log_count: g.log_count,
            sample_messages: g.sample_messages,
            relative_change: 0.0, // TODO: Calculate from baseline
        }).collect())
    }

    /// Store template and return the assigned template_id
    /// If template_id is 0, generates next available ID from ClickHouse
    pub async fn insert_template(&self, mut template: TemplateRow) -> Result<u64> {
        // If template_id is 0, get next available ID
        if template.template_id == 0 {
            template.template_id = self.get_next_template_id().await?;
        }

        let mut insert = self.client.insert("templates")?;
        insert.write(&template).await?;
        insert.end().await?;

        Ok(template.template_id)
    }

    /// Get next available template ID from ClickHouse
    async fn get_next_template_id(&self) -> Result<u64> {
        #[derive(Debug, clickhouse::Row, Deserialize)]
        struct MaxIdRow {
            max_id: u64,
        }

        let result = self.client
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

    /// Insert a template example
    pub async fn insert_template_example(&self, log: &LogEntry) -> Result<()> {
        if log.template_id.is_empty() {
            return Ok(()); // Skip logs without templates
        }

        #[derive(Serialize)]
        struct TemplateExample {
            org_id: String,
            log_stream_id: String,
            service: String,
            region: String,
            template_id: String,
            message: String,
            timestamp: String,
        }

        let example = TemplateExample {
            org_id: log.org_id.clone(),
            log_stream_id: log.log_stream_id.clone(),
            service: log.service.clone(),
            region: log.region.clone(),
            template_id: log.template_id.clone(),
            message: log.message.clone(),
            timestamp: log.timestamp.format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
        };

        let json_line = serde_json::to_string(&example)?;

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
        org_id: &str,
        log_stream_id: &str,
        template_id: &str,
        limit: usize,
    ) -> Result<Vec<LogEntry>> {
        let query = format!(
            "SELECT org_id, log_stream_id, service, region, template_id, message, timestamp
             FROM template_examples
             WHERE org_id = '{}'
               AND log_stream_id = '{}'
               AND template_id = '{}'
             ORDER BY timestamp DESC
             LIMIT {}
             FORMAT JSONEachRow",
            org_id, log_stream_id, template_id, limit
        );

        let http_client = reqwest::Client::new();
        let response = http_client
            .post(&self.url)
            .body(query)
            .send()
            .await?;

        let body = response.text().await?;

        #[derive(Deserialize)]
        struct TemplateExampleRow {
            org_id: String,
            log_stream_id: String,
            service: String,
            region: String,
            template_id: String,
            message: String,
            timestamp: String,
        }

        let examples: Vec<LogEntry> = body
            .lines()
            .filter_map(|line| {
                let row: TemplateExampleRow = serde_json::from_str(line).ok()?;
                Some(LogEntry {
                    org_id: row.org_id,
                    log_stream_id: row.log_stream_id,
                    service: row.service,
                    region: row.region,
                    log_stream_name: String::new(), // Not stored in template_examples
                    timestamp: DateTime::parse_from_str(&row.timestamp, "%Y-%m-%d %H:%M:%S%.3f")
                        .ok()?
                        .with_timezone(&Utc),
                    template_id: row.template_id,
                    message: row.message,
                })
            })
            .collect();

        Ok(examples)
    }

    /// Insert template with auto-generated ID (alias for insert_template)
    pub async fn insert_template_with_autoid(&self, template: TemplateRow) -> Result<u64> {
        self.insert_template(template).await
    }

    /// Get templates for a specific org and log stream
    pub async fn get_templates_for_stream(&self, org_id: &str, log_stream_id: &str) -> Result<Vec<TemplateRow>> {
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
        self.client.query("TRUNCATE TABLE templates").execute().await?;
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
            org_id: "org-1".to_string(),
            log_stream_id: "stream-1".to_string(),
            service: "api-server".to_string(),
            region: "us-east-1".to_string(),
            log_stream_name: "/aws/api/production".to_string(),
            timestamp: Utc::now(),
            template_id: "template-1".to_string(),
            message: "Test error message".to_string(),
        };

        client.insert_log(log.clone()).await.unwrap();

        let logs = client
            .query_logs("org-1", "stream-1", Utc::now() - chrono::Duration::hours(1), Utc::now())
            .await
            .unwrap();

        assert!(!logs.is_empty());
    }
}
