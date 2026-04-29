//! Buffered ClickHouse writer that fully decouples the hot path from
//! ClickHouse I/O.
//!
//! Hot path: an HTTP handler calls `write(entry).await`, which is a single
//! channel send — never blocks on a database round-trip, never holds a
//! mutex during flush.
//!
//! Writer task: a single dedicated task drains the channel into local
//! batches and flushes to ClickHouse. Flushes happen on either size or
//! time trigger. The hot path's latency is independent of ClickHouse's
//! behavior — under load, the channel absorbs short bursts and applies
//! backpressure on sustained overload.
//!
//! Trade-off: on a hard process crash, entries that have been accepted
//! by the channel but not yet flushed to ClickHouse are lost. The
//! channel capacity caps the worst-case loss.

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
    debug!(
        "Flushing {} logs to ClickHouse ({} trigger)",
        count, trigger
    );
    if let Err(e) = clickhouse.insert_logs_batch(logs).await {
        error!("Failed to flush {} logs to ClickHouse: {}", count, e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn entry(id: usize) -> LogEntry {
        LogEntry {
            org_id: "o".into(),
            log_stream_id: "s".into(),
            service: "svc".into(),
            region: "r".into(),
            log_stream_name: "n".into(),
            timestamp: Utc::now(),
            template_id: format!("t{id}"),
            message: format!("msg-{id}"),
        }
    }

    /// Stand up a wiremock server that 200s every POST and counts how
    /// many times each insert_logs_batch hit it. Returns the server +
    /// the per-flush hit counter.
    async fn ch_server() -> (MockServer, std::sync::Arc<AtomicUsize>) {
        let server = MockServer::start().await;
        let counter = std::sync::Arc::new(AtomicUsize::new(0));
        let counter_for_mock = counter.clone();
        Mock::given(method("POST"))
            .respond_with(move |_: &wiremock::Request| {
                counter_for_mock.fetch_add(1, Ordering::SeqCst);
                ResponseTemplate::new(200)
            })
            .mount(&server)
            .await;
        (server, counter)
    }

    #[tokio::test]
    async fn write_batch_with_empty_vec_is_a_noop() {
        let (server, hits) = ch_server().await;
        let ch = Arc::new(ClickHouseClient::new(&server.uri()).unwrap());
        let writer = Arc::new(BufferedClickHouseWriter::new(
            ch,
            10,
            Duration::from_secs(60),
        ));
        let _handle = writer.clone().start_background_flusher();

        writer.write_batch(vec![]).await; // empty

        // Give the writer a moment to wake up; it should not have flushed.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(hits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn size_trigger_flushes_when_threshold_reached() {
        let (server, hits) = ch_server().await;
        let ch = Arc::new(ClickHouseClient::new(&server.uri()).unwrap());
        // batch size 3, very long interval so only the size trigger fires.
        let writer = Arc::new(BufferedClickHouseWriter::new(
            ch,
            3,
            Duration::from_secs(60),
        ));
        let _handle = writer.clone().start_background_flusher();

        // Send 3 entries split across two batches; cumulative ≥ 3 → flush.
        writer.write_batch(vec![entry(1), entry(2)]).await;
        writer.write_batch(vec![entry(3)]).await;

        // Poll for the flush — one POST expected.
        for _ in 0..50 {
            if hits.load(Ordering::SeqCst) >= 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(hits.load(Ordering::SeqCst) >= 1);
    }

    #[tokio::test]
    async fn time_trigger_flushes_partial_batch() {
        let (server, hits) = ch_server().await;
        let ch = Arc::new(ClickHouseClient::new(&server.uri()).unwrap());
        // huge batch size so only the time trigger fires.
        let writer = Arc::new(BufferedClickHouseWriter::new(
            ch,
            10_000,
            Duration::from_millis(80),
        ));
        let _handle = writer.clone().start_background_flusher();

        writer.write(entry(1)).await; // single entry — under size threshold

        // Wait for the time trigger to fire at least once.
        for _ in 0..50 {
            if hits.load(Ordering::SeqCst) >= 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
        assert!(hits.load(Ordering::SeqCst) >= 1);
    }

    #[tokio::test]
    async fn write_single_record_via_write_method() {
        // The non-batched write() helper wraps a single record in a Vec
        // and routes through write_batch. Verifying both code paths.
        let (server, hits) = ch_server().await;
        let ch = Arc::new(ClickHouseClient::new(&server.uri()).unwrap());
        let writer = Arc::new(BufferedClickHouseWriter::new(
            ch,
            1, // size 1 → flush immediately
            Duration::from_secs(60),
        ));
        let _handle = writer.clone().start_background_flusher();

        writer.write(entry(1)).await;

        for _ in 0..50 {
            if hits.load(Ordering::SeqCst) >= 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(hits.load(Ordering::SeqCst) >= 1);
    }

    #[tokio::test]
    async fn flush_does_not_panic_on_clickhouse_error() {
        // ClickHouse returns 500; the writer should log and keep going,
        // not poison the channel or panic the task.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let ch = Arc::new(ClickHouseClient::new(&server.uri()).unwrap());
        let writer = Arc::new(BufferedClickHouseWriter::new(
            ch,
            1,
            Duration::from_secs(60),
        ));
        let _handle = writer.clone().start_background_flusher();

        writer.write(entry(1)).await;

        // Give the failing flush time to complete; sender should still
        // be usable for subsequent writes.
        tokio::time::sleep(Duration::from_millis(150)).await;
        writer.write(entry(2)).await; // must not panic
    }

    #[tokio::test]
    #[should_panic(expected = "start_background_flusher called more than once")]
    async fn double_start_panics() {
        let (server, _) = ch_server().await;
        let ch = Arc::new(ClickHouseClient::new(&server.uri()).unwrap());
        let writer = Arc::new(BufferedClickHouseWriter::new(
            ch,
            10,
            Duration::from_secs(60),
        ));
        let _h1 = writer.clone().start_background_flusher();
        // The second call must panic because the receiver was already taken.
        let _h2 = writer.clone().start_background_flusher();
    }
}
