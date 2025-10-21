# LLM-Assisted Hierarchical Template Generation Workflow

## Complete Workflow

### Step 1: LLM Analyzes Sample Log
**Input**: One representative log from each event type

```
Sample log: "Jun 15 02:04:59 combo sshd(pam_unix)[20882]: authentication failure; user=root rhost=220-135-151-1.hinet-ip.hinet.net"
```

**LLM classifies each field:**
```json
{
  "log_type_signature": "sshd pam_unix authentication failure user",
  "template_signature": "sshd pam_unix authentication failure user=<User> rhost=<Location>",
  "regex_pattern": "^[A-Z][a-z]{2}\\s+\\d{1,2}\\s+\\d{2}:\\d{2}:\\d{2}\\s+([\\w.-]+)\\s+sshd\\(pam_unix\\)\\[(\\d+)\\]:\\s+authentication\\s+failure;.*user=([\\w]+).*rhost=([\\w.-]+)",
  "fields": [
    {"field": "Jun", "classification": "EPHEMERAL", "reason": "Month"},
    {"field": "15", "classification": "EPHEMERAL", "reason": "Day"},
    {"field": "02:04:59", "classification": "EPHEMERAL", "reason": "Time"},
    {"field": "combo", "classification": "EPHEMERAL", "reason": "Hostname instance"},
    {"field": "sshd", "classification": "STATIC", "reason": "Service name"},
    {"field": "pam_unix", "classification": "STATIC", "reason": "Module name"},
    {"field": "20882", "classification": "EPHEMERAL", "reason": "PID"},
    {"field": "authentication", "classification": "STATIC", "reason": "Action"},
    {"field": "failure", "classification": "STATIC", "reason": "Status"},
    {"field": "user=", "classification": "STATIC", "reason": "Field marker"},
    {"field": "root", "classification": "PARAMETER", "parameter_type": "User", "reason": "Username for clustering"},
    {"field": "rhost=", "classification": "STATIC", "reason": "Field marker"},
    {"field": "220-135-151-1.hinet-ip.hinet.net", "classification": "PARAMETER", "parameter_type": "Location", "reason": "Source host"}
  ]
}
```

### Step 2: Extract Signatures

**Level 1 - Log Type** (STATIC fields only):
```
"sshd pam_unix authentication failure user rhost"
```
This becomes the **LogType ID**. All auth failures with user field map here.

**Level 2 - Template** (STATIC + PARAMETER types):
```
"sshd pam_unix authentication failure user=<User> rhost=<Location>"
```
This becomes the **Template ID** for matching.

### Step 3: Generate Regex for Fast Matching

LLM provides optimized regex:
```regex
^[A-Z][a-z]{2}\s+\d{1,2}\s+\d{2}:\d{2}:\d{2}\s+[\w.-]+\s+sshd\(pam_unix\)\[\d+\]:\s+authentication\s+failure;.*user=(?P<username>[\w]+).*rhost=(?P<rhost>[\w.-]+)
```

**Named capture groups** for parameter extraction:
- `(?P<username>[\w]+)` → extracts username value
- `(?P<rhost>[\w.-]+)` → extracts rhost value

### Step 4: Match New Logs (No LLM Needed!)

```rust
// Incoming log
let log = "Jun 16 10:30:45 server1 sshd(pam_unix)[30123]: authentication failure; user=admin rhost=attacker.com";

// Match against templates using regex (fast!)
if let Some(captures) = template.regex.captures(log) {
    let username = captures.name("username").unwrap().as_str(); // "admin"
    let rhost = captures.name("rhost").unwrap().as_str(); // "attacker.com"

    // Record match
    record_match(template_id, {
        "username": "admin",
        "rhost": "attacker.com"
    });
}
```

### Step 5: Track Distributions

```rust
// Time window: Last hour
let distributions = DistributionTracker {
    log_type_counts: {
        "sshd_auth_failure_user": 450  // 45% of logs
    },

    param_distributions: {
        "sshd_auth_failure_user": {
            "username": {
                "root": 10,      // 2.2%  ← ANOMALY! Usually 94%
                "admin": 200,    // 44.4% ← ATTACK!
                "oracle": 150,   // 33.3% ← ATTACK!
                "mysql": 90      // 20.0% ← ATTACK!
            },
            "rhost": {
                "attacker.com": 380,    // 84.4% ← SINGLE SOURCE!
                "malicious.net": 70     // 15.6%
            }
        }
    }
};
```

### Step 6: Compute KL Divergence

**Baseline** (normal traffic, trained over 1 week):
```python
P_baseline(log_type) = {
    "sshd_auth_failure_user": 0.18,  # 18% of logs
    "ftpd_connection": 0.45,
    ...
}

P_baseline(username | auth_failure) = {
    "root": 0.942,
    "guest": 0.047,
    "test": 0.011
}
```

**Current** (last hour - attack):
```python
P_current(log_type) = {
    "sshd_auth_failure_user": 0.45,  # 45%! ← SPIKE
    "ftpd_connection": 0.30,
    ...
}

P_current(username | auth_failure) = {
    "admin": 0.444,   # NEW!
    "oracle": 0.333,  # NEW!
    "mysql": 0.200,   # NEW!
    "root": 0.022
}
```

**KL Divergence Computation:**
```python
# Template-level
D_KL(P_baseline || P_current) at log type level:
  = Σ P_baseline(x) * log(P_baseline(x) / P_current(x))
  = 0.18 * log(0.18/0.45) + ...
  = 0.32  # Moderate divergence

# Parameter-level (username)
D_KL(P_baseline || P_current) for username:
  = 0.942 * log(0.942/0.022) + 0.047 * log(0.047/0) + ...
  = 3.58  # VERY HIGH DIVERGENCE!

# Combined Alert
if template_kl > 0.3 and param_kl > 2.0:
    alert("ATTACK DETECTED: Brute force SSH attack on multiple admin accounts")
```

## Full System Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│ Phase 1: Template Discovery (One-time, uses LLM)               │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  1. Sample one log per event type                              │
│  2. LLM classifies fields → STATIC/EPHEMERAL/PARAMETER         │
│  3. LLM generates regex pattern with named capture groups      │
│  4. Store template in database                                 │
│                                                                 │
│  Result: ~20-50 templates for entire log corpus                │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│ Phase 2: Production Matching (Fast, NO LLM)                    │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Incoming Log                                                   │
│      ↓                                                          │
│  Regex Matching (try all templates)                            │
│      ↓                                                          │
│  Extract Parameters (named capture groups)                     │
│      ↓                                                          │
│  Update Distributions                                           │
│      ↓                                                          │
│  Compute KL Divergence (every N minutes)                       │
│      ↓                                                          │
│  Alert if divergence > threshold                               │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Key Benefits

### 1. LLM Only for Discovery
- Run once per log type
- Generate templates offline
- No LLM API calls in production

### 2. Fast Production Matching
- Pure regex matching (1M+ logs/sec)
- Named capture groups for parameter extraction
- No parsing ambiguity

### 3. Automatic Template IDs
**Question**: "How do we know which template ID to use?"

**Answer**: The template_signature IS the template ID!

```rust
// Template storage
HashMap<String, Template> templates = {
    "sshd pam_unix authentication failure user=<User>": Template {
        id: 1,
        regex: ...,
        parameters: ["User", "Location"]
    },
    "ftpd connection from=<IP>": Template {
        id: 2,
        regex: ...,
        parameters: ["IP", "Hostname"]
    }
};

// Matching
for (sig, template) in templates {
    if template.regex.is_match(log) {
        return template.id;
    }
}
```

### 4. Ground Truth Comparison

**Ground Truth**:
- E16: auth failure (no user) - 117 logs
- E17: auth failure user=guest - 17 logs
- E18: auth failure user=root - 341 logs
- E19: auth failure user=test - 4 logs

**Our System**:
- Template 1: auth failure (no user) - 117 logs
- Template 2: auth failure user=<User> - 362 logs
  - Parameter distribution: {root: 94.2%, guest: 4.7%, test: 1.1%}

**For Evaluation**:
```rust
fn map_to_ground_truth(template_id: u64, params: HashMap<String, String>) -> String {
    match (template_id, params.get("username")) {
        (1, None) => "E16",  // No user field
        (2, Some("root")) => "E18",
        (2, Some("guest")) => "E17",
        (2, Some("test")) => "E19",
        _ => "Unknown"
    }
}
```

### 5. Novel Value Handling

```
New log: "authentication failure; user=admin"

Ground Truth:
  - Creates E120 (new template)
  - Can't compute KL (template set changed)

Our System:
  - Matches Template 2 (auth failure user=<User>)
  - Extracts username="admin"
  - Updates distribution: {root: 93%, guest: 5%, admin: 1%, test: 1%}
  - KL divergence detects anomaly!
```

## Implementation Checklist

- [x] Token classification (rule-based)
- [x] Hierarchical matching demo
- [x] Dataset analysis
- [ ] LLM template generation (fix JSON parsing)
- [ ] Regex-based matcher with named groups
- [ ] Distribution tracker with time windows
- [ ] KL divergence computation
- [ ] Alert threshold configuration
- [ ] Ground truth mapping for evaluation

## Next Steps

1. Fix LLM JSON parsing (handle large responses)
2. Generate templates for all Linux log types (~44 templates needed)
3. Build fast regex matcher
4. Implement distribution tracking
5. Test KL divergence on simulated attacks
