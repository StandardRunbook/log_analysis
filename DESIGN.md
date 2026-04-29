# Design notes

Properties of the ingest pipeline that aren't visible from outside but are
load-bearing for correctness and throughput.

## Content-hashed `template_id`

`template_id = blake3(canonicalize(pattern))[..8]`.

Stable across restarts, deploys, and database migrations: the same parse
tree always maps to the same ID. Concurrent template synthesis is
naturally idempotent — two workers that produce the same canonical
pattern compute the same ID and the duplicate row is squashed by
ClickHouse's `ReplacingMergeTree`. Necessary for divergence-over-time
correctness: aggregates from two windows can be compared directly only
if the same template ID denotes the same shape across both.

## No orphan log rows

A row in the `logs` table is only inserted once its `template_id` is
known. Misses are queued in full (the original log line plus all
resource attributes); the LLM consumer writes the row after rematching
or LLM synthesis. The schema does not contain rows with empty
`template_id`.

The alternative — write the row with an empty/sentinel ID, then
backfill — was rejected because it makes the per-template counts that
divergence analytics consume stale until backfill completes, and
because the backfill query competes with the hot-path writer for the
same partition.

## Decoupled writer with channel batching

The hot path performs one channel send per **request** (a
`Vec<LogEntry>` carrying every matched log from that request), not one
send per record. A dedicated writer task drains the channel into
ClickHouse on size-or-time triggers. Two consequences:

- Hot-path latency is independent of ClickHouse round-trip time. A
  ClickHouse hiccup grows the channel queue but doesn't propagate
  latency to the OTLP responder.
- Receiver wakeups are coalesced from per-record to per-request
  frequency. At hundreds of thousands of logs per second the
  difference between waking the writer task once per record vs. once
  per request is significant.

The crash trade-off: records accepted by the channel but not yet
flushed are lost on hard process termination. Channel capacity caps
worst-case loss.

## Single-consumer LLM worker with rematch-before-LLM dedup

Novel log shapes flow through a single mpsc consumer. Before sending
any record to the LLM, the consumer rematches it against the current
matcher snapshot. This dedups bursts: if N records of the same novel
shape arrive simultaneously, the first one triggers an LLM call; by
the time the second is pulled from the queue, the matcher has been
updated with the new template and the rematch returns a hit. N
records → 1 LLM call.

A multi-consumer design would scale better in CPU but produce N LLM
calls in this same scenario, which dominates cost at provider rates.

## Architecture diagram

```
                ┌──────────────────────────────────────────────────────────┐
                │                  ingest service (one process)             │
                │                                                          │
   OTel         │   ┌──────────────────┐                                   │
   collector ───┼──▶│  OTLP/gRPC :4317 │──▶ LogMatcher (parse tree)        │
                │   └──────────────────┘    - Aho-Corasick DFA             │
                │                           - ArcSwap snapshot              │
                │                           - per-tenant in v1+             │
                │                           ┌─────┴──────┐                  │
                │                       (HIT)         (MISS)                │
                │                           │             │                 │
                │              ┌────────────▼──┐  ┌───────▼───────────┐    │
                │              │ writer channel│  │ unmatched mpsc    │    │
                │              │ (Vec<Entry>)  │  │ (LogEntry)        │    │
                │              └──────┬────────┘  └───────┬───────────┘    │
                │                     │                   │                 │
                │       ┌─────────────▼──────┐  ┌─────────▼────────┐       │
                │       │  Buffered writer   │  │ LLM consumer     │       │
                │       │  task (decoupled)  │  │ task (single)    │       │
                │       │  - batch + timer   │  │ - rematch dedup  │       │
                │       └────────┬───────────┘  │ - LLM synthesis  │       │
                │                │              │ - own cold-path  │       │
                │                │              │   write          │       │
                │                │              └────────┬─────────┘       │
                │                │                       │                  │
                │   ┌────────────▼──┐    ┌───────────────▼────┐            │
                │   │ HTTP :3002    │    └────────────┬───────┘            │
                │   │ /health,/stats│                 │                     │
                │   └───────────────┘                 │                     │
                └─────────────────────────────────────┼─────────────────────┘
                                                      ▼
                                              ┌──────────────┐
                                              │  ClickHouse  │
                                              │  logs +      │
                                              │  templates   │
                                              └──────────────┘
```
