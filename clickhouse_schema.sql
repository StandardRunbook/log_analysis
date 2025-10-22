-- ClickHouse Schema for Log Analysis Service
-- This schema stores logs with their matched templates for efficient querying

-- Main logs table
CREATE TABLE IF NOT EXISTS logs (
    timestamp DateTime64(3) CODEC(DoubleDelta, LZ4),
    org String CODEC(LZ4),
    dashboard String CODEC(LZ4),
    service String CODEC(LZ4),
    host String CODEC(LZ4),
    level String CODEC(LZ4),
    message String CODEC(LZ4),
    template_id Nullable(UInt64) CODEC(LZ4),
    template_pattern Nullable(String) CODEC(LZ4),

    -- Metadata (flexible JSON for customer data)
    metadata String DEFAULT '{}' CODEC(LZ4),

    -- Ingestion metadata
    ingested_at DateTime DEFAULT now() CODEC(DoubleDelta, LZ4)
)
ENGINE = MergeTree()
PARTITION BY toYYYYMMDD(timestamp)
ORDER BY (org, timestamp, template_id)
TTL timestamp + INTERVAL 30 DAY  -- Keep logs for 30 days
SETTINGS index_granularity = 8192;

-- Templates table (for reference)
CREATE TABLE IF NOT EXISTS templates (
    template_id UInt64,
    pattern String,
    variables Array(String),
    example String,
    created_at DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(created_at)
ORDER BY template_id
SETTINGS index_granularity = 8192;

-- Materialized view for fast template grouping
CREATE MATERIALIZED VIEW IF NOT EXISTS logs_by_template_hourly
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMMDD(hour)
ORDER BY (org, dashboard, hour, template_id)
AS SELECT
    org,
    dashboard,
    toStartOfHour(timestamp) as hour,
    template_id,
    count() as log_count,
    min(timestamp) as first_seen,
    max(timestamp) as last_seen,
    groupArray(5)(message) as sample_messages  -- Keep 5 sample messages
FROM logs
GROUP BY org, dashboard, hour, template_id;

-- Index for fast Grafana queries
CREATE TABLE IF NOT EXISTS log_summary (
    org String,
    dashboard String,
    panel_title String,
    metric_name String,
    hour DateTime,
    template_id UInt64,
    log_count UInt64,
    sample_messages Array(String),
    first_timestamp DateTime,
    last_timestamp DateTime
)
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMMDD(hour)
ORDER BY (org, dashboard, panel_title, metric_name, hour, template_id)
TTL hour + INTERVAL 7 DAY  -- Keep summaries for 7 days
SETTINGS index_granularity = 8192;
