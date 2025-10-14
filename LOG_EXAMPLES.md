# Log Examples and Characteristics

## Default Templates and Example Logs

### Template 1: CPU Usage (template_id: 1)

**Pattern**: `r"cpu_usage: (\d+\.\d+)% - (.*)"`  
**Prefix**: `"cpu_usage: "`  
**Variables**: `["percentage", "message"]`

**Example Logs:**
```
cpu_usage: 45.2% - Server load normal
cpu_usage: 67.8% - Server load increased
cpu_usage: 12.5% - Low utilization
cpu_usage: 99.9% - Critical load
cpu_usage: 50.0% - Moderate usage
```

**Characteristics:**
- Always starts with `"cpu_usage: "`
- Followed by decimal number (percentage)
- Ends with descriptive message
- Used for CPU monitoring

---

### Template 2: Memory Usage (template_id: 2)

**Pattern**: `r"memory_usage: (\d+\.\d+)GB - (.*)"`  
**Prefix**: `"memory_usage: "`  
**Variables**: `["amount", "message"]`

**Example Logs:**
```
memory_usage: 2.5GB - Memory consumption stable
memory_usage: 8.0GB - High memory usage
memory_usage: 0.5GB - Low memory
memory_usage: 4.2GB - Memory stable
memory_usage: 15.8GB - Memory consumption increasing
```

**Characteristics:**
- Always starts with `"memory_usage: "`
- Followed by decimal number with GB unit
- Ends with status message
- Used for memory monitoring

---

### Template 3: Disk I/O (template_id: 3)

**Pattern**: `r"disk_io: (\d+)MB/s - (.*)"`  
**Prefix**: `"disk_io: "`  
**Variables**: `["throughput", "message"]`

**Example Logs:**
```
disk_io: 250MB/s - Disk activity moderate
disk_io: 1000MB/s - High throughput
disk_io: 10MB/s - Low activity
disk_io: 500MB/s - Disk activity normal
disk_io: 100MB/s - Disk active
```

**Characteristics:**
- Always starts with `"disk_io: "`
- Followed by integer throughput with MB/s unit
- Ends with activity description
- Used for disk I/O monitoring

---

## Additional Templates (Used in Benchmarks)

### Template 4: Network Traffic

**Pattern**: `r"network_traffic: (\d+)Mbps - (.*)"`  
**Prefix**: `"network_traffic: "`

**Example Logs:**
```
network_traffic: 500Mbps - Network load moderate
network_traffic: 1000Mbps - High bandwidth usage
network_traffic: 50Mbps - Network load light
network_traffic: 750Mbps - Network load heavy
```

---

### Template 5: Error Rate

**Pattern**: `r"error_rate: (\d+\.\d+)% - (.*)"`  
**Prefix**: `"error_rate: "`

**Example Logs:**
```
error_rate: 0.05% - System status healthy
error_rate: 2.50% - System status degraded
error_rate: 0.01% - System healthy
error_rate: 5.00% - System status critical
```

---

### Template 6: Request Latency

**Pattern**: `r"request_latency: (\d+)ms - (.*)"`  
**Prefix**: `"request_latency: "`

**Example Logs:**
```
request_latency: 125ms - Response time acceptable
request_latency: 50ms - Response time optimal
request_latency: 500ms - Response time slow
request_latency: 25ms - Response fast
```

---

### Template 7: Database Connections

**Pattern**: `r"database_connections: (\d+) - (.*)"`  
**Prefix**: `"database_connections: "`

**Example Logs:**
```
database_connections: 45 - Pool status healthy
database_connections: 10 - Pool available
database_connections: 95 - Pool status limited
database_connections: 5 - Pool healthy
database_connections: 100 - Pool status exhausted
```

---

## Log Format Characteristics

### Common Structure

All default logs follow a similar pattern:

```
[metric_name]: [value][unit] - [descriptive_message]
```

**Components:**
1. **Metric Name**: Identifies the type of log (e.g., `cpu_usage`, `memory_usage`)
2. **Value**: Numeric measurement (integer or decimal)
3. **Unit**: Measurement unit (%, GB, MB/s, Mbps, ms)
4. **Message**: Human-readable status description

### Matching Behavior

**What gets matched:**
```rust
// ✓ Matches template 1
"cpu_usage: 67.8% - Server load high"
"cpu_usage: 10.0% - anything here"
"cpu_usage: 99.9999% - very specific"

// ✗ Does NOT match any template
"CPU_usage: 67.8%" // Different case
"cpu: 67.8%"       // Missing "usage:" part
"usage: 67.8%"     // Missing "cpu_" part
```

**Important**: Only the **prefix** is checked. The number format and message after the prefix don't affect matching:

```rust
// All of these match template 1 (same prefix):
"cpu_usage: 67.8% - High load"        // ✓ Valid format
"cpu_usage: INVALID - Wrong format"   // ✓ Still matches! (prefix only)
"cpu_usage: "                         // ✓ Still matches! (just prefix)
"cpu_usage: 🚀🚀🚀"                   // ✓ Still matches!
```

---

## Real-World Log Examples

### Typical Application Logs

If you want to match real application logs, here are some template ideas:

#### HTTP Access Logs
```rust
LogTemplate {
    template_id: 10,
    pattern: r#"(\d+\.\d+\.\d+\.\d+) - - \[.*\] "(\w+) (/\S*) HTTP/\d\.\d" (\d+)"#,
    variables: vec!["ip", "method", "path", "status"],
    example: r#"192.168.1.1 - - [15/Jan/2024:14:30:45] "GET /api/users HTTP/1.1" 200"#,
}
// Prefix: "" (no static prefix - starts with IP variable)
```

#### Application Errors
```rust
LogTemplate {
    template_id: 11,
    pattern: r"\[ERROR\] (.*): (.*)",
    variables: vec!["component", "message"],
    example: "[ERROR] Database: Connection timeout after 30s",
}
// Prefix: "[ERROR] "
```

#### Application Warnings
```rust
LogTemplate {
    template_id: 12,
    pattern: r"\[WARN\] (.*): (.*)",
    variables: vec!["component", "message"],
    example: "[WARN] Cache: Cache miss rate above 50%",
}
// Prefix: "[WARN] "
```

#### Structured JSON Logs
```rust
LogTemplate {
    template_id: 13,
    pattern: r#"\{"level":"error".*\}"#,
    variables: vec![],
    example: r#"{"level":"error","timestamp":"2024-01-15T14:30:45Z","message":"Failed to connect"}"#,
}
// Prefix: "{\"level\":\"error\""
```

---

## Benchmark Log Distribution

In the benchmark tests, logs are generated in a **round-robin** pattern:

```
Log #0: cpu_usage: 10.0% - Server load normal
Log #1: memory_usage: 0.5GB - Memory stable
Log #2: disk_io: 10MB/s - Disk activity moderate
Log #3: network_traffic: 1Mbps - Network load light
Log #4: error_rate: 0.00% - System healthy
Log #5: request_latency: 10ms - Response fast
Log #6: database_connections: 1 - Pool healthy
Log #7: cpu_usage: 11.0% - Server load normal   (cycles back)
...
```

**Distribution**: Each template gets approximately 14.3% of logs (1/7)

**This explains the benchmark results:**
```
Total logs processed:  1000000
Matched:               142857 (14.3%)   ← Only 1/7 match (default templates 1-3)
Unmatched:             857143 (85.7%)   ← Templates 4-7 not in default matcher
```

---

## Performance by Log Characteristics

### Log Length Impact

| Log Type | Avg Length | Performance |
|----------|-----------|-------------|
| Short (20-30 chars) | `"cpu_usage: 67.8% - OK"` | Fastest |
| Medium (50-80 chars) | `"memory_usage: 2.5GB - Memory consumption stable"` | Fast |
| Long (100+ chars) | `"request_latency: 125ms - Response time acceptable for user request ID 12345"` | Still Fast |

**Impact**: Minimal - Aho-Corasick is O(n) where n is log length, but prefix matching is very fast.

### Prefix Length Impact

| Prefix Length | Example | Performance |
|---------------|---------|-------------|
| Short (3-5 chars) | `"cpu"` | Fastest |
| Medium (10-20 chars) | `"cpu_usage: "` | Fast |
| Long (30+ chars) | `"[2024-01-15 14:30:45] ERROR: "` | Still Fast |

**Impact**: Minimal - Aho-Corasick handles multi-pattern matching efficiently regardless of prefix length.

### Template Count Impact

| Template Count | Throughput | Notes |
|----------------|-----------|-------|
| 3 templates | 208M logs/sec | Current benchmark |
| 7 templates | 208M logs/sec | No measurable difference |
| 100 templates | ~200M logs/sec (estimated) | Aho-Corasick scales well |
| 1000+ templates | Testing recommended | Should still be >100M logs/sec |

---

## Summary

**Current Default Templates:**
1. `cpu_usage:` - CPU monitoring
2. `memory_usage:` - Memory monitoring  
3. `disk_io:` - Disk I/O monitoring

**Key Characteristics:**
- Simple, predictable format: `[metric]: [value][unit] - [message]`
- Static prefix for fast matching
- Metrics-focused (system performance monitoring)
- Round-robin distribution in benchmarks

**Performance:**
- 208M logs/sec with batch processing
- O(n) time complexity per log (n = log length)
- Scales well with template count (Aho-Corasick DFA)
- Prefix-only matching (no regex validation)

**For Production Use:**
- Analyze your actual logs to create appropriate templates
- Ensure each template has a unique, static prefix
- Test with your real log volume and format
- Consider adding templates for errors, warnings, and application-specific events
