# LLM Template Generation Guide

This document explains how the system generates log templates using an LLM, with emphasis on properly masking ephemeral (changing) fields.

## Overview

When a log line doesn't match any existing template in the radix trie, the system sends it to an LLM to generate a new template. The LLM must create a regex pattern that:

1. **Matches similar logs** - Pattern should work for variations of the same log type
2. **Masks ephemeral fields** - Replace changing values with capture groups
3. **Preserves static text** - Keep constant parts unchanged
4. **Extracts meaningful data** - Use descriptive variable names

## What Are Ephemeral Fields?

Ephemeral fields are values that change between log occurrences but represent the same type of log. These MUST be masked with regex capture groups.

### Common Ephemeral Fields

| Field Type | Example | Regex Pattern | Variable Name |
|------------|---------|---------------|---------------|
| **ISO Timestamp** | `2025-01-15T10:30:45Z` | `(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z\|[+-]\d{2}:\d{2})?)` | `timestamp` |
| **IP Address** | `192.168.1.100` | `(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})` | `ip_address` |
| **UUID** | `550e8400-e29b-41d4-a716-446655440000` | `([a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})` | `uuid` |
| **Request ID** | `req_abc123xyz` | `([a-zA-Z0-9_-]+)` | `request_id` |
| **User ID** | `user_12345` | `([a-zA-Z0-9_]+)` | `user_id` |
| **Session ID** | `sess_a1b2c3d4` | `([a-zA-Z0-9_-]+)` | `session_id` |
| **Decimal Number** | `45.23` | `(\d+\.\d+)` | `decimal_value` |
| **Integer** | `5000` | `(\d+)` | `numeric_value` |
| **Percentage** | `85.5%` | `(\d+\.?\d*)%` | `percentage` |
| **File Path** | `/var/log/app.log` | `([/\w\.-]+)` | `file_path` |
| **URL** | `https://api.example.com/v1/users` | `(https?://[^\s]+)` | `url` |
| **Duration** | `123ms`, `5.5s` | `(\d+\.?\d*)(ms\|s\|m\|h)` | `duration` + `duration_unit` |
| **Byte Size** | `2.5MB` | `(\d+\.?\d*)(B\|KB\|MB\|GB\|TB)` | `size` + `size_unit` |
| **HTTP Status** | `200`, `404` | `(\d{3})` | `status_code` |
| **Error Code** | `ERR_1234` | `([A-Z_-]+\d+)` | `error_code` |
| **Hostname** | `db.example.com` | `([a-zA-Z0-9\.-]+)` | `hostname` |
| **Port** | `5432` | `(\d+)` | `port` |
| **JSON Object** | `{"status":"ok"}` | `(\{.*\})` | `json_response` |

## Template Generation Examples

### Example 1: User Login Log

**Original Log:**
```
2025-01-15T10:30:45Z [INFO] User user_12345 logged in from 192.168.1.100
```

**Generated Template:**
```regex
(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z) \[INFO\] User ([a-zA-Z0-9_]+) logged in from (\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})
```

**Variables:**
```json
["timestamp", "user_id", "ip_address"]
```

**Explanation:**
- ✅ Masked: timestamp, user_id, ip_address
- ✅ Kept: `[INFO] User`, `logged in from`
- ✅ Will match: Similar login logs with different timestamps, user IDs, and IPs

---

### Example 2: HTTP Request Log

**Original Log:**
```
Request req_abc123 completed in 145ms with status 200
```

**Generated Template:**
```regex
Request ([a-zA-Z0-9_]+) completed in (\d+)ms with status (\d{3})
```

**Variables:**
```json
["request_id", "duration_ms", "status_code"]
```

**Explanation:**
- ✅ Masked: request_id, duration, status code
- ✅ Kept: `Request`, `completed in`, `ms with status`
- ✅ Specific: Won't match unrelated completion messages

---

### Example 3: Database Error Log

**Original Log:**
```
ERROR: Database connection failed - host: db.example.com:5432, error: timeout after 30s
```

**Generated Template:**
```regex
ERROR: Database connection failed - host: ([a-zA-Z0-9\.-]+):(\d+), error: timeout after (\d+)s
```

**Variables:**
```json
["hostname", "port", "timeout_seconds"]
```

**Explanation:**
- ✅ Masked: hostname, port, timeout value
- ✅ Kept: `ERROR: Database connection failed - host:`, `error: timeout after`, `s`
- ✅ Flexible: Matches different hosts, ports, and timeout values

---

### Example 4: Metric Log

**Original Log:**
```
cpu_usage: 67.8% - Server load increased
```

**Generated Template:**
```regex
cpu_usage: (\d+\.?\d*)% - (.*)
```

**Variables:**
```json
["percentage", "message"]
```

**Explanation:**
- ✅ Masked: percentage value, message text
- ✅ Kept: `cpu_usage:`, `%`, `-`
- ✅ Flexible: Matches any percentage and any message

---

### Example 5: File Processing Log

**Original Log:**
```
Processing file /var/log/app-2025-01-15.log size: 2.5MB
```

**Generated Template:**
```regex
Processing file ([/\w\.-]+) size: (\d+\.?\d*)(MB|GB|KB)
```

**Variables:**
```json
["file_path", "size_value", "size_unit"]
```

**Explanation:**
- ✅ Masked: file path, size value, size unit
- ✅ Kept: `Processing file`, `size:`
- ✅ Separated: Size value and unit in separate groups for flexibility

---

### Example 6: API Call Log

**Original Log:**
```
API call to https://api.service.com/v1/users/123 returned {"status":"success","count":5}
```

**Generated Template:**
```regex
API call to (https?://[^\s]+) returned (\{.*\})
```

**Variables:**
```json
["url", "response_json"]
```

**Explanation:**
- ✅ Masked: Full URL, JSON response
- ✅ Kept: `API call to`, `returned`
- ✅ Flexible: Matches any URL and JSON response

---

### Example 7: Transaction Log

**Original Log:**
```
Transaction txn_a1b2c3d4 from account_12345 to account_67890 amount: $150.00 status: completed
```

**Generated Template:**
```regex
Transaction ([a-zA-Z0-9_]+) from account_(\d+) to account_(\d+) amount: \$(\d+\.\d{2}) status: (\w+)
```

**Variables:**
```json
["transaction_id", "from_account", "to_account", "amount", "status"]
```

**Explanation:**
- ✅ Masked: transaction ID, account numbers, amount, status
- ✅ Kept: All structural text including `$` symbol
- ✅ Specific: Amount format enforces 2 decimal places

---

### Example 8: Thread Warning Log

**Original Log:**
```
WARN [2025-01-15 10:30:45] Thread pool-3-thread-7 - Queue size: 1024/2048 (50%)
```

**Generated Template:**
```regex
WARN \[(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})\] Thread ([a-zA-Z0-9\-]+) - Queue size: (\d+)/(\d+) \((\d+)%\)
```

**Variables:**
```json
["timestamp", "thread_name", "current_size", "max_size", "percentage"]
```

**Explanation:**
- ✅ Masked: timestamp, thread name, queue metrics, percentage
- ✅ Kept: `WARN`, `Thread`, `Queue size:`, parentheses
- ✅ Escaped: `[`, `]`, `(`, `)` in static text

## Common Mistakes to Avoid

### ❌ Mistake 1: Not Masking Ephemeral Fields

**Bad:**
```regex
User user_12345 logged in
```

**Good:**
```regex
User ([a-zA-Z0-9_]+) logged in
```

**Why:** The pattern should match ANY user ID, not just `user_12345`.

---

### ❌ Mistake 2: Too Generic Pattern

**Bad:**
```regex
(.*)
```

**Good:**
```regex
Request ([a-zA-Z0-9_]+) completed in (\d+)ms
```

**Why:** Too generic patterns match unrelated logs. Be specific about structure.

---

### ❌ Mistake 3: Wrong Variable Count

**Bad:**
```regex
User (\w+) logged in from (\d+\.\d+\.\d+\.\d+)
Variables: ["user_id"]  # Only 1 variable for 2 capture groups!
```

**Good:**
```regex
User (\w+) logged in from (\d+\.\d+\.\d+\.\d+)
Variables: ["user_id", "ip_address"]
```

**Why:** Number of variables must match number of capture groups.

---

### ❌ Mistake 4: Not Escaping Special Characters

**Bad:**
```regex
ERROR: Connection failed. Retry in 5s
```

**Good:**
```regex
ERROR: Connection failed\. Retry in (\d+)s
```

**Why:** `.` is a special regex character (matches any character). Escape it with `\.`

---

### ❌ Mistake 5: Generic Variable Names

**Bad:**
```regex
(\d+)ms with status (\d+)
Variables: ["value1", "value2"]
```

**Good:**
```regex
(\d+)ms with status (\d+)
Variables: ["duration_ms", "status_code"]
```

**Why:** Use semantic names that describe what the field represents.

## Integration with Production LLM

To integrate with a real LLM service (OpenAI, Anthropic, etc.):

### 1. Uncomment the API call method in `llm_service.rs`

```rust
async fn call_llm_api(&self, request: &TemplateGenerationRequest) -> Result<LogTemplate> {
    let url = format!("{}/api/generate-template", self.base_url);
    
    let response = self.client
        .post(&url)
        .json(&request)
        .send()
        .await?
        .error_for_status()?;
    
    let llm_response: TemplateGenerationResponse = response.json().await?;
    Ok(llm_response.template)
}
```

### 2. Build the prompt for your LLM

The `TemplateGenerationRequest` includes:
- **instructions**: Detailed rules for template generation
- **examples**: 8 examples of good templates
- **log_line**: The actual log line to process

### 3. Example OpenAI Integration

```rust
async fn call_openai(&self, request: &TemplateGenerationRequest) -> Result<LogTemplate> {
    let prompt = format!(
        "{}\n\nEXAMPLES:\n{}\n\nNow generate a template for this log:\n{}",
        request.instructions,
        serde_json::to_string_pretty(&request.examples)?,
        request.log_line
    );
    
    let openai_request = json!({
        "model": "gpt-4",
        "messages": [
            {
                "role": "system",
                "content": "You are a log template generator that creates regex patterns for log parsing."
            },
            {
                "role": "user",
                "content": prompt
            }
        ],
        "temperature": 0.3,
        "response_format": { "type": "json_object" }
    });
    
    let response = self.client
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(&self.api_key)
        .json(&openai_request)
        .send()
        .await?;
    
    // Parse response and extract template
    // ...
}
```

### 4. Example Anthropic Claude Integration

```rust
async fn call_claude(&self, request: &TemplateGenerationRequest) -> Result<LogTemplate> {
    let prompt = format!(
        "{}\n\nEXAMPLES:\n{}\n\nLog line to process:\n{}",
        request.instructions,
        serde_json::to_string_pretty(&request.examples)?,
        request.log_line
    );
    
    let claude_request = json!({
        "model": "claude-3-5-sonnet-20250131",
        "max_tokens": 1024,
        "messages": [{
            "role": "user",
            "content": prompt
        }]
    });
    
    let response = self.client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &self.api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&claude_request)
        .send()
        .await?;
    
    // Parse response and extract template
    // ...
}
```

## Template Validation

After receiving a template from the LLM, validate it:

```rust
fn validate_template(template: &LogTemplate, original_log: &str) -> Result<()> {
    // 1. Compile regex
    let regex = Regex::new(&template.pattern)?;
    
    // 2. Check it matches the original log
    let captures = regex.captures(original_log)
        .ok_or(anyhow!("Pattern doesn't match original log"))?;
    
    // 3. Verify variable count matches capture groups
    if captures.len() - 1 != template.variables.len() {
        return Err(anyhow!("Variable count mismatch"));
    }
    
    Ok(())
}
```

## Performance Considerations

1. **Cache templates**: Store generated templates in database to avoid regenerating
2. **Batch requests**: Send multiple logs to LLM at once if possible
3. **Fallback**: Have a simple heuristic generator if LLM is unavailable
4. **Rate limiting**: Implement backoff if hitting LLM API rate limits
5. **Cost tracking**: Monitor LLM API costs per template generation

## Monitoring

Track these metrics:
- Templates generated per hour
- Template quality (false positives/negatives)
- LLM API latency
- LLM API cost
- Template reuse rate (how often new templates are actually needed)
