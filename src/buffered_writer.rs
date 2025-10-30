/// Buffered ClickHouse writer with smart batching
///
/// Flushes based on:
/// - Buffer size (e.g., 1000 logs)
/// - Time window (e.g., 5 seconds)
/// - Graceful shutdown signal

use crate::clickhouse_client::{ClickHouseClient, LogEntry};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::Instant;
use tracing::{debug, error, info};

pub struct BufferedClickHouseWriter {
    clickhouse: Arc<ClickHouseClient>,
    buffer: Arc<Mutex<Vec<LogEntry>>>,
    max_buffer_size: usize,
    flush_interval: Duration,
}

impl BufferedClickHouseWriter {
    pub fn new(
        clickhouse: Arc<ClickHouseClient>,
        max_buffer_size: usize,
        flush_interval: Duration,
    ) -> Self {
        Self {
            clickhouse,
            buffer: Arc::new(Mutex::new(Vec::with_capacity(max_buffer_size))),
            max_buffer_size,
            flush_interval,
        }
    }

    /// Add a log entry to the buffer
    /// Returns true if buffer was flushed
    pub async fn write(&self, log: LogEntry) -> bool {
        let mut buffer = self.buffer.lock().await;
        buffer.push(log);

        // Check if we need to flush based on size
        if buffer.len() >= self.max_buffer_size {
            let logs_to_flush = buffer.drain(..).collect::<Vec<_>>();
            drop(buffer); // Release lock before async call

            debug!("Flushing {} logs to ClickHouse (size trigger)", logs_to_flush.len());
            if let Err(e) = self.clickhouse.insert_logs_batch(logs_to_flush).await {
                error!("Failed to flush logs to ClickHouse: {}", e);
            }
            return true;
        }

        false
    }

    /// Start background flusher task
    /// Flushes periodically based on time window
    pub fn start_background_flusher(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(self.flush_interval);
            let mut last_flush = Instant::now();

            loop {
                interval.tick().await;

                let mut buffer = self.buffer.lock().await;
                if !buffer.is_empty() {
                    let elapsed = last_flush.elapsed();
                    let logs_to_flush = buffer.drain(..).collect::<Vec<_>>();
                    let count = logs_to_flush.len();
                    drop(buffer); // Release lock

                    debug!(
                        "Flushing {} logs to ClickHouse (time trigger, {}ms since last flush)",
                        count,
                        elapsed.as_millis()
                    );

                    if let Err(e) = self.clickhouse.insert_logs_batch(logs_to_flush).await {
                        error!("Failed to flush logs to ClickHouse: {}", e);
                    } else {
                        last_flush = Instant::now();
                    }
                }
            }
        })
    }

    /// Force flush all buffered logs
    pub async fn flush(&self) -> anyhow::Result<()> {
        let mut buffer = self.buffer.lock().await;
        if buffer.is_empty() {
            return Ok(());
        }

        let logs_to_flush = buffer.drain(..).collect::<Vec<_>>();
        let count = logs_to_flush.len();
        drop(buffer);

        info!("Force flushing {} logs to ClickHouse", count);
        self.clickhouse.insert_logs_batch(logs_to_flush).await
    }
}
