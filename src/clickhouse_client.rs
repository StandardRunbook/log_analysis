/// ClickHouse client for log storage
///
/// Provides high-performance log ingestion and querying using ClickHouse

use anyhow::Result;
use clickhouse::Client;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, clickhouse::Row)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub org: String,
    pub dashboard: String,
    pub service: String,
    pub host: String,
    pub level: String,
    pub message: String,
    pub template_id: Option<u64>,
    pub template_pattern: Option<String>,
    pub metadata: String,
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
}

impl ClickHouseClient {
    /// Create a new ClickHouse client
    pub fn new(url: &str) -> Result<Self> {
        let client = Client::default()
            .with_url(url)
            .with_database("default");

        Ok(Self { client })
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

        let mut insert = self.client.insert("logs")?;
        for log in logs {
            insert.write(&log).await?;
        }
        insert.end().await?;

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
