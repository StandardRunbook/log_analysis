# Log Template Guide

## Overview

The LogMatcher uses **templates** to identify and classify log lines. Each template defines a pattern that logs can match against.

## LogTemplate Structure

```rust
pub struct LogTemplate {
    pub template_id: u64,        // Unique identifier for this template
    pub pattern: String,          // Regex pattern (used for prefix extraction only)
    pub variables: Vec<String>,   // Variable names (currently not used for extraction)
    pub example: String,          // Example log that matches this template
}
```

**Important Note**: Although the structure includes `pattern` (regex) and `variables`, the current implementation **only uses the prefix** for matching. Regex validation and variable extraction have been removed for performance.

## Current Default Templates

### Template 1: CPU Usage
```rust
LogTemplate {
    template_id: 1,
    pattern: r"cpu_usage: (\d+\.\d+)% - (.*)",
    variables: vec!["percentage", "message"],
    example: "cpu_usage: 45.2% - Server load normal",
}
```

**Prefix used for matching**: `"cpu_usage: "`

**Matches logs like:**
- `cpu_usage: 67.8% - Server load increased`
- `cpu_usage: 12.3% - Low utilization`
- `cpu_usage: 99.9% - Critical load`

### Template 2: Memory Usage
```rust
LogTemplate {
    template_id: 2,
    pattern: r"memory_usage: (\d+\.\d+)GB - (.*)",
    variables: vec!["amount", "message"],
    example: "memory_usage: 2.5GB - Memory consumption stable",
}
```

**Prefix used for matching**: `"memory_usage: "`

**Matches logs like:**
- `memory_usage: 2.5GB - Memory consumption stable`
- `memory_usage: 8.0GB - High memory usage`
- `memory_usage: 0.5GB - Low memory`

### Template 3: Disk I/O
```rust
LogTemplate {
    template_id: 3,
    pattern: r"disk_io: (\d+)MB/s - (.*)",
    variables: vec!["throughput", "message"],
    example: "disk_io: 250MB/s - Disk activity moderate",
}
```

**Prefix used for matching**: `"disk_io: "`

**Matches logs like:**
- `disk_io: 250MB/s - Disk activity moderate`
- `disk_io: 1000MB/s - High throughput`
- `disk_io: 10MB/s - Low activity`

## How Matching Works

### 1. Prefix Extraction

When a template is added, the system extracts a **static prefix** from the pattern:

```rust
fn extract_prefix(pattern: &str) -> String {
    // Take characters up to the first regex metacharacter
    pattern
        .chars()
        .take_while(|c| !matches!(c, '(' | '[' | '.' | '*' | '+' | '?' | '\\'))
        .collect()
}
```

**Examples:**
- Pattern: `r"cpu_usage: (\d+\.\d+)% - (.*)"` → Prefix: `"cpu_usage: "`
- Pattern: `r"memory_usage: (\d+\.\d+)GB - (.*)"` → Prefix: `"memory_usage: "`
- Pattern: `r"error: (.*)"`→ Prefix: `"error: "`

### 2. Aho-Corasick DFA

All prefixes are compiled into an **Aho-Corasick deterministic finite automaton**:

```rust
// Build DFA from all prefixes
let prefixes = vec!["cpu_usage: ", "memory_usage: ", "disk_io: "];
let ac = AhoCorasick::new(&prefixes)?;

// Match in O(n) time - finds ALL matching prefixes in one pass
if let Some(match) = ac.find(log_line) {
    let template_id = pattern_to_template[match.pattern().as_usize()];
    return Some(template_id);
}
```

### 3. Matching Process

**Single log:**
```rust
let template_id = matcher.match_log("cpu_usage: 67.8% - High load");
// Returns: Some(1)
```

**Batch:**
```rust
let results = matcher.match_batch(&[
    "cpu_usage: 67.8% - High load",
    "memory_usage: 2.5GB - Stable",
    "disk_io: 100MB/s - Active",
]);
// Returns: vec![Some(1), Some(2), Some(3)]
```

## Template Characteristics

### What Makes a Good Template?

1. **Unique Static Prefix**
   - ✅ Good: `"cpu_usage: "`, `"[ERROR] "`, `"2024-01-01 "`
   - ❌ Bad: `"."` (too generic), `""` (empty)

2. **Common Log Patterns**
   - System metrics (CPU, memory, disk)
   - Application errors and warnings
   - HTTP requests/responses
   - Database queries
   - Security events

3. **Performance Considerations**
   - Shorter prefixes = faster matching
   - More specific prefixes = fewer false positives
   - Static text before variables = better performance

### Template Examples

**System Metrics:**
```rust
LogTemplate {
    template_id: 4,
    pattern: r"network_traffic: (\d+)Mbps - (.*)",
    variables: vec!["throughput", "status"],
    example: "network_traffic: 500Mbps - Network load moderate",
}
// Prefix: "network_traffic: "
```

**Error Logs:**
```rust
LogTemplate {
    template_id: 5,
    pattern: r"\[ERROR\] (.*): (.*)",
    variables: vec!["component", "message"],
    example: "[ERROR] Database: Connection timeout",
}
// Prefix: "[ERROR] "
```

**HTTP Requests:**
```rust
LogTemplate {
    template_id: 6,
    pattern: r"(\w+) (/\S+) HTTP/(\d\.\d) (\d+)",
    variables: vec!["method", "path", "version", "status"],
    example: "GET /api/users HTTP/1.1 200",
}
// Prefix: "" (no static prefix before first variable - will match any log starting with word chars)
```

**Timestamps:**
```rust
LogTemplate {
    template_id: 7,
    pattern: r"\[(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})\] (.*)",
    variables: vec!["timestamp", "message"],
    example: "[2024-01-15 14:30:45] Application started",
}
// Prefix: "[" (stops at first regex metacharacter)
```

## Current Limitations

### 1. Prefix-Only Matching

The system currently **only matches on prefixes**, not full regex patterns:

```rust
// Template pattern: r"cpu_usage: (\d+\.\d+)% - (.*)"
// Actually used: "cpu_usage: "

// These will ALL match template 1:
matcher.match_log("cpu_usage: 67.8% - High load");     // ✓ Intended match
matcher.match_log("cpu_usage: INVALID - Wrong format"); // ✓ Also matches! (prefix only)
matcher.match_log("cpu_usage: ");                       // ✓ Also matches!
```

**Why?** Regex validation was removed for 19x performance improvement. Pure Aho-Corasick prefix matching is much faster.

### 2. No Variable Extraction

The `variables` field is currently **not used**:

```rust
// Variables are defined but not extracted
let template_id = matcher.match_log("cpu_usage: 67.8% - High load");
// Returns: Some(1)
// Does NOT return: { "percentage": "67.8", "message": "High load" }
```

**Why?** Value extraction was removed for 100x+ performance improvement. The system now only returns template IDs.

### 3. First Match Wins

If multiple templates have overlapping prefixes, the **first match wins**:

```rust
// Template A: prefix = "error"
// Template B: prefix = "error:"
// Template C: prefix = "error: database"

// Log: "error: database connection failed"
// Matches: Template A (shortest prefix found first)
```

## Adding Custom Templates

### Method 1: At Runtime

```rust
let mut matcher = LogMatcher::new();

matcher.add_template(LogTemplate {
    template_id: 100,
    pattern: r"[ERROR] (.*): (.*)".to_string(),
    variables: vec!["component".to_string(), "message".to_string()],
    example: "[ERROR] Database: Connection timeout".to_string(),
});

// Now can match error logs
let result = matcher.match_log("[ERROR] Database: Connection timeout");
// Returns: Some(100)
```

### Method 2: Modify Default Templates

Edit `src/log_matcher.rs`:

```rust
impl LogMatcher {
    pub fn new() -> Self {
        let mut snapshot = MatcherSnapshot::new();

        let default_templates = vec![
            // Your templates here
            LogTemplate {
                template_id: 1,
                pattern: r"your_pattern_here".to_string(),
                variables: vec!["var1".to_string()],
                example: "your example log".to_string(),
            },
        ];
        
        // ...
    }
}
```

## Best Practices

### 1. Design Templates for Your Log Format

Analyze your actual logs and create templates that match:

```bash
# Sample your logs
head -1000 application.log | cut -d' ' -f1-3 | sort | uniq -c | sort -rn

# Example output:
#  450 [ERROR] Database:
#  320 [WARN] Cache:
#  180 [INFO] API:
#   50 cpu_usage: 67.8%
```

### 2. Use Specific Prefixes

More specific = fewer false positives:

```rust
// ❌ Too generic
pattern: r"(\d+)" // Prefix: "" (empty - matches anything)

// ✅ Better
pattern: r"Response time: (\d+)ms" // Prefix: "Response time: "
```

### 3. Group Similar Logs

One template per log type:

```rust
// ✅ Good: One template for all HTTP requests
pattern: r"HTTP (\w+) (/\S+) (\d+)"

// ❌ Bad: Separate templates for GET, POST, etc.
pattern: r"HTTP GET (/\S+) (\d+)"
pattern: r"HTTP POST (/\S+) (\d+)"
```

### 4. Test Your Templates

```rust
#[test]
fn test_custom_template() {
    let mut matcher = LogMatcher::new();
    
    matcher.add_template(LogTemplate {
        template_id: 200,
        pattern: r"\[CUSTOM\] (.*)".to_string(),
        variables: vec!["message".to_string()],
        example: "[CUSTOM] Test message".to_string(),
    });
    
    assert_eq!(
        matcher.match_log("[CUSTOM] Test message"),
        Some(200)
    );
}
```

## Performance Implications

### Template Count

- **1-100 templates**: Excellent performance (208M logs/sec)
- **100-1000 templates**: Good performance (~150M+ logs/sec)
- **1000+ templates**: May need testing, but Aho-Corasick scales well

### Prefix Length

- **Short prefixes (3-10 chars)**: Fastest matching
- **Long prefixes (50+ chars)**: Slightly slower but still fast
- **No prefix (starts with variable)**: Slowest (matches everything)

### Pattern Complexity

Since we **only use prefixes**, pattern complexity doesn't matter:

```rust
// These have identical performance:
pattern: r"cpu: (\d+)"
pattern: r"cpu: (\d+\.\d+)%? - (.*) at (\d{4}-\d{2}-\d{2})"
// Both use prefix: "cpu: "
```

## Summary

- **Templates** define log patterns with unique IDs
- **Matching** uses only the static prefix (before first regex metacharacter)
- **Performance** is 208M logs/sec with batch processing
- **Current system** returns template IDs only (no variable extraction)
- **Best practice** is to use specific, unique prefixes for each log type
