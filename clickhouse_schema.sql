-- ClickHouse Schema for Log Analysis Service
-- Two-table design optimized for KL divergence and anomaly detection

-- Table 1: Logs for histogram/KL divergence queries
-- Ordered by (org, dashboard, panel, metric, time) for fast range queries
CREATE TABLE IF NOT EXISTS logs (
    timestamp DateTime CODEC(DoubleDelta, LZ4),
    org String CODEC(LZ4),
    dashboard String CODEC(LZ4),
    panel_name String CODEC(LZ4),
    metric_name String CODEC(LZ4),
    service String CODEC(LZ4),
    host String CODEC(LZ4),
    level String CODEC(LZ4),
    message String CODEC(LZ4),
    template_id Nullable(UInt64) CODEC(LZ4),
    template_pattern Nullable(String) CODEC(LZ4),
    metadata String CODEC(LZ4)
)
ENGINE = MergeTree()
PARTITION BY toYYYYMMDD(timestamp)
ORDER BY (org, dashboard, panel_name, metric_name, timestamp)
TTL timestamp + INTERVAL 30 DAY  -- Keep logs for 30 days
SETTINGS index_granularity = 8192;

-- Table 2: Template examples for showing representative logs to users
-- Ordered by (template_id first) for fast lookup when showing anomaly context
-- Examples rotate every hour to keep them fresh
CREATE TABLE IF NOT EXISTS template_examples (
    template_id UInt64,
    org String,
    dashboard String,
    panel_name String,
    metric_name String,
    timestamp DateTime,
    service String,
    host String,
    level String,
    message String,
    template_pattern String,
    metadata String,
    added_at DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(added_at)
PARTITION BY template_id
ORDER BY (template_id, org, dashboard, panel_name, metric_name, timestamp)
TTL added_at + INTERVAL 1 HOUR  -- Keep examples for 1 hour, then rotate
SETTINGS index_granularity = 8192;
