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

/// Channel capacity is in *batches* (`Vec<LogEntry>`), not individual
/// records. Each batch typically carries one HTTP/OTLP request's worth
/// of logs, so 10K batches is plenty of headroom for bursts.
const CHANNEL_CAPACITY: usize = 10_000;

pub struct BufferedClickHouseWriter {
    tx: mpsc::Sender<Vec<LogEntry>>,
    // Receiver is taken at `start_background_flusher` time; holding it in
    // the struct keeps `new()` synchronous while letting the writer task
    // own the receiver after spawn.
    rx: Mutex<Option<mpsc::Receiver<Vec<LogEntry>>>>,
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

    /// Send a single log record. Wraps it in a single-element batch
    /// internally; cheap, but prefer `write_batch` from any hot-path
    /// caller that already has many records grouped (HTTP/OTLP request
    /// handlers).
    pub async fn write(&self, log: LogEntry) {
        self.write_batch(vec![log]).await
    }

    /// Hot-path entry for batched callers: send a whole request's worth
    /// of records in one channel op. Compared to `write` per record,
    /// this collapses N channel ops + N receiver wakeups into 1, which
    /// matters at hundreds-of-K logs/sec.
    pub async fn write_batch(&self, logs: Vec<LogEntry>) {
        if logs.is_empty() {
            return;
        }
        if let Err(e) = self.tx.send(logs).await {
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
    mut rx: mpsc::Receiver<Vec<LogEntry>>,
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
                    let to_flush = std::mem::replace(&mut batch, Vec::with_capacity(max_batch_size));
                    flush(&clickhouse, to_flush, "time").await;
                }
            }

            // Drain incoming batches from the channel.
            recv = rx.recv() => {
                match recv {
                    Some(mut incoming) => {
                        batch.append(&mut incoming);
                        // If this push tipped us over the size threshold,
                        // flush whatever we have. We don't try to slice at
                        // exactly max_batch_size — a slightly-larger flush
                        // is cheaper than the bookkeeping to split it.
                        if batch.len() >= max_batch_size {
                            let to_flush = std::mem::replace(
                                &mut batch,
                                Vec::with_capacity(max_batch_size),
                            );
                            flush(&clickhouse, to_flush, "size").await;
                        }
                    }
                    None => {
                        // Channel closed — drain remaining entries and exit.
                        if !batch.is_empty() {
                            let to_flush = std::mem::take(&mut batch);
                            flush(&clickhouse, to_flush, "shutdown").await;
                        }
                        break;
                    }
                }
            }
        }
    }
}

async fn flush(clickhouse: &ClickHouseClient, logs: Vec<LogEntry>, trigger: &str) {
    let count = logs.len();
    debug!("Flushing {} logs to ClickHouse ({} trigger)", count, trigger);
    if let Err(e) = clickhouse.insert_logs_batch(logs).await {
        error!("Failed to flush {} logs to ClickHouse: {}", count, e);
    }
}
