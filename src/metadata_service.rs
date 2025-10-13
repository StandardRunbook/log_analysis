use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct MetadataQuery {
    pub org: String,
    pub dashboard: String,
    pub graph_name: String,
    pub metric_name: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LogStream {
    pub stream_id: String,
    pub stream_name: String,
    pub source: String,
}

#[derive(Debug, Deserialize)]
pub struct MetadataResponse {
    pub log_streams: Vec<LogStream>,
}

pub struct MetadataServiceClient {
    base_url: String,
    client: reqwest::Client,
}

impl MetadataServiceClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: reqwest::Client::new(),
        }
    }

    /// Query the metadata service to get relevant log streams for a metric
    pub async fn get_log_streams(&self, query: &MetadataQuery) -> Result<Vec<LogStream>> {
        let _url = format!("{}/api/log-streams", self.base_url);

        tracing::info!(
            "Querying metadata service for org: {}, dashboard: {}, graph: {}, metric: {} in time range {} to {}",
            query.org,
            query.dashboard,
            query.graph_name,
            query.metric_name,
            query.start_time,
            query.end_time
        );

        // In production, this would make a real HTTP call
        // For now, return mock data based on metric name
        Ok(self.mock_metadata_response(
            &query.org,
            &query.dashboard,
            &query.graph_name,
            &query.metric_name,
        ))
    }

    /// Mock implementation - replace with actual API call in production
    fn mock_metadata_response(
        &self,
        org: &str,
        dashboard: &str,
        graph_name: &str,
        metric_name: &str,
    ) -> Vec<LogStream> {
        tracing::info!(
            "Mock metadata lookup: org='{}', dashboard='{}', graph='{}', metric='{}'",
            org,
            dashboard,
            graph_name,
            metric_name
        );

        match metric_name {
            "cpu_usage" => vec![
                LogStream {
                    stream_id: format!("{}/{}/stream-001", org, dashboard),
                    stream_name: "system-metrics-primary".to_string(),
                    source: "server-01".to_string(),
                },
                LogStream {
                    stream_id: format!("{}/{}/stream-002", org, dashboard),
                    stream_name: "system-metrics-secondary".to_string(),
                    source: "server-02".to_string(),
                },
            ],
            "memory_usage" => vec![LogStream {
                stream_id: format!("{}/{}/stream-003", org, dashboard),
                stream_name: "memory-monitor".to_string(),
                source: "monitoring-service".to_string(),
            }],
            "disk_io" => vec![LogStream {
                stream_id: format!("{}/{}/stream-004", org, dashboard),
                stream_name: "disk-performance".to_string(),
                source: "storage-monitor".to_string(),
            }],
            // For any unknown metric, return sample data so Grafana gets a response
            _ => {
                tracing::warn!(
                    "Unknown metric '{}', returning default sample data. Available metrics: cpu_usage, memory_usage, disk_io",
                    metric_name
                );
                vec![
                    LogStream {
                        stream_id: format!("{}/{}/stream-default-001", org, dashboard),
                        stream_name: "system-metrics-primary".to_string(),
                        source: "server-01".to_string(),
                    },
                    LogStream {
                        stream_id: format!("{}/{}/stream-default-002", org, dashboard),
                        stream_name: "system-metrics-secondary".to_string(),
                        source: "server-02".to_string(),
                    },
                ]
            }
        }
    }

    // Uncomment this for actual API integration
    /*
    async fn query_api(&self, query: &MetadataQuery) -> Result<Vec<LogStream>> {
        let url = format!("{}/api/log-streams", self.base_url);

        let response = self.client
            .post(&url)
            .json(query)
            .send()
            .await?
            .error_for_status()?;

        let metadata_response: MetadataResponse = response.json().await?;
        Ok(metadata_response.log_streams)
    }
    */
}
