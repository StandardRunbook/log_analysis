/// Buffered ClickHouse writer that fully decouples the hot path from
/// ClickHouse I/O.
///
/// Hot path: an HTTP handler calls `write(entry).await`, which is a single
/// channel send — never blocks on a database round-trip, never holds a
/// mutex during flush.
///
/// Writer task: a single dedicated task drains the channel into local
/// batches and flushes to ClickHouse. Flushes happen on either size or
/// time trigger. The hot path's latency is independent of ClickHouse's
/// behavior — under load, the channel absorbs short bursts and applies
/// backpressure on sustained overload.
///
/// Trade-off: on a hard process crash, entries that have been accepted
/// by the channel but not yet flushed to ClickHouse are lost. The
/// channel capacity caps the worst-case loss.

use crate::clickhouse_client::{ClickHouseClient, LogEntry};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use tokio::time::MissedTickBehavior;
use tracing::{debug, error, info, warn};

/// Channel capacity for buffered writes. Must be large enough to absorb
/// brief bursts above ClickHouse's write rate without applying
/// backpressure to HTTP handlers. At 1000 logs per batch and ~10ms per
/// flush, 100K is roughly 1 second of headroom at full burst.
const CHANNEL_CAPACITY: usize = 100_000;

pub struct BufferedClickHouseWriter {
    tx: mpsc::Sender<LogEntry>,
    // Receiver is taken at `start_background_flusher` time; holding it in
    // the struct keeps `new()` synchronous while letting the writer task
    // own the receiver after spawn.
    rx: Mutex<Option<mpsc::Receiver<LogEntry>>>,
    clickhouse: Arc<ClickHouseClient>,
    max_batch_size: usize,
    flush_interval: Duration,
}

impl BufferedClickHouseWriter {
    pub fn new(
        clickhouse: Arc<ClickHouseClient>,
        max_batch_size: usize,
        flush_interval: Duration,
    ) -> Self {
        let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);
        Self {
            tx,
            rx: Mutex::new(Some(rx)),
            clickhouse,
            max_batch_size,
            flush_interval,
        }
    }

    /// Hot-path entry. Sends the log into the writer's channel; awaits
    /// only if the channel is at capacity (sustained overload), in which
    /// case the caller experiences natural backpressure.
    pub async fn write(&self, log: LogEntry) {
        if let Err(e) = self.tx.send(log).await {
            error!("BufferedClickHouseWriter: channel closed: {}", e);
        }
    }

    /// Spawn the writer task. Call once at startup. The returned handle
    /// must be kept alive (or `_` bound) to keep the task running.
    pub fn start_background_flusher(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let rx = self
            .rx
            .try_lock()
            .expect("start_background_flusher called concurrently")
            .take()
            .expect("start_background_flusher called more than once");

        let clickhouse = self.clickhouse.clone();
        let max_batch_size = self.max_batch_size;
        let flush_interval = self.flush_interval;

        tokio::spawn(async move {
            run_writer_task(rx, clickhouse, max_batch_size, flush_interval).await;
            warn!("BufferedClickHouseWriter task stopped");
        })
    }
}

async fn run_writer_task(
    mut rx: mpsc::Receiver<LogEntry>,
    clickhouse: Arc<ClickHouseClient>,
    max_batch_size: usize,
    flush_interval: Duration,
) {
    info!(
        "BufferedClickHouseWriter task started (batch={}, interval={:?})",
        max_batch_size, flush_interval
    );

    let mut batch: Vec<LogEntry> = Vec::with_capacity(max_batch_size);
    let mut ticker = tokio::time::interval(flush_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    // First tick fires immediately; consume it so we don't flush an empty batch.
    ticker.tick().await;

    loop {
        tokio::select! {
            // Time-based flush. Always evaluated; fires regardless of how many
            // entries have arrived since the last tick.
            _ = ticker.tick() => {
                if !batch.is_empty() {
                    flush(&clickhouse, &mut batch, "time").await;
                }
            }

            // Drain entries from the channel.
            recv = rx.recv() => {
                match recv {
                    Some(entry) => {
                        batch.push(entry);
                        if batch.len() >= max_batch_size {
                            flush(&clickhouse, &mut batch, "size").await;
                        }
                    }
                    None => {
                        // Channel closed — drain remaining entries and exit.
                        if !batch.is_empty() {
                            flush(&clickhouse, &mut batch, "shutdown").await;
                        }
                        break;
                    }
                }
            }
        }
    }
}

async fn flush(clickhouse: &ClickHouseClient, batch: &mut Vec<LogEntry>, trigger: &str) {
    let logs = std::mem::take(batch);
    let count = logs.len();
    debug!("Flushing {} logs to ClickHouse ({} trigger)", count, trigger);
    if let Err(e) = clickhouse.insert_logs_batch(logs).await {
        error!("Failed to flush {} logs to ClickHouse: {}", count, e);
    }
}
