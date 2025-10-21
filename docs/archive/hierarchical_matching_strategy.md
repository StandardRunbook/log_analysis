# Hierarchical Template Matching Strategy

## Overview

Three-tier token classification for intelligent log template matching:

```
STATIC tokens     → Log Type (Level 1)
STATIC + PARAMETER → Template ID (Level 2)
All tokens        → Parameter extraction (Level 3)
```

## Token Classification

### 1. STATIC Tokens
**Definition**: Keywords that define log structure and never change.

**Examples**:
- Service names: `sshd`, `kernel`, `nginx`, `postgres`
- Action verbs: `authentication`, `failure`, `opened`, `closed`
- Field markers: `uid=`, `user=`, `rhost=`, `status=`

**Purpose**: Identify the **type** of log event

### 2. EPHEMERAL Tokens
**Definition**: Values that always change and have no semantic clustering value.

**Examples**:
- Timestamps: `Jun`, `14`, `15:16:01`, `2024-01-15`
- IDs: PIDs `19939`, request IDs, UUIDs
- Network: IP addresses `192.168.1.1`, ports `8080`
- Counters: sequence numbers, message counts

**Purpose**: **Ignore** for template matching (noise)

### 3. PARAMETER Tokens
**Definition**: Business-relevant values that cluster logs into template variants.

**Categories**:
- **User**: `root`, `guest`, `admin`, `test`
- **Resource**: `/var/log`, `database.table`, `service_name`
- **Action**: `GET`, `POST`, `DELETE`, error codes
- **Location**: hostnames (not IPs)

**Purpose**: Define template **variants** and track distributions

## Two-Level Matching

### Level 1: Log Type Identification
**Input**: STATIC tokens only
**Output**: Log type signature

**Example**:
```
Log: Jun 14 15:16:01 combo sshd(pam_unix)[19939]: authentication failure; uid=0 rhost=192.168.1.1

Tokens:
  STATIC: [sshd, pam_unix, authentication, failure, uid, rhost]
  EPHEMERAL: [Jun, 14, 15, 16, 01, 19939, 192.168.1.1]
  PARAMETER: [combo:Generic]

Level 1 Signature: "sshd pam_unix authentication failure uid rhost"
```

**Result**: All auth failures (regardless of username) map to same log type.

### Level 2: Template Clustering
**Input**: STATIC + PARAMETER tokens
**Output**: Template signature with parameter types

**Example**:
```
E17: authentication failure; user=guest rhost=192.168.1.1
E18: authentication failure; user=root rhost=example.com
E19: authentication failure; user=test rhost=10.0.0.1

All have Level 1: "sshd authentication failure uid rhost user"

But Level 2 distinguishes:
  Template signature: "sshd authentication failure uid rhost <Location> user <User>"

Parameter tracking:
  username: {root: 90%, guest: 8%, test: 2%}
  rhost_type: {ip: 70%, hostname: 30%}
```

## Implementation Flow

```rust
// 1. Tokenize log
let tokens = tokenize(log_line);

// 2. Classify each token
let classified: Vec<(String, TokenClass)> = tokens
    .iter()
    .map(|t| (t.clone(), classify_token(t, context)))
    .collect();

// 3. Extract Level 1 signature (STATIC only)
let log_type = extract_log_type_signature(&classified);
// → "sshd authentication failure"

// 4. Find or create log type ID
let log_type_id = get_or_create_log_type(log_type);

// 5. Extract Level 2 signature (STATIC + PARAMETER types)
let template_sig = extract_template_signature(&classified);
// → "sshd authentication failure user=<User> rhost=<Location>"

// 6. Find or create template ID
let template_id = get_or_create_template(log_type_id, template_sig);

// 7. Extract parameter values
let params = extract_parameters(&classified);
// → {username: "root", rhost: "example.com"}

// 8. Record match
record_match(log_type_id, template_id, params);
```

## Benefits for KL Divergence

### Problem with Current Approach
Ground truth creates 117 templates for 1999 logs because:
- `user=root` → E18 (351 logs)
- `user=guest` → E17 (17 logs)
- `user=test` → E19 (4 logs)

**Issue**: Can't tell if spike is "more auth failures" or "more root logins"

### Hierarchical Approach Solution

**Level 1 Distribution** (Log Types):
```
P(log_type) = {
  auth_failure: 40%,
  ftp_connection: 30%,
  session_opened: 20%,
  ...
}
```
**Detects**: Structural changes in system behavior

**Level 2 Distribution** (Parameters per Log Type):
```
P(username | auth_failure) = {
  root: 90%,
  guest: 8%,
  test: 2%
}

P(rhost_type | auth_failure) = {
  ip: 70%,
  hostname: 30%
}
```
**Detects**: Behavioral changes within same log type

### KL Divergence Computation

**Template-level**:
```python
baseline = {auth: 0.40, ftp: 0.30, session: 0.30}
current  = {auth: 0.80, ftp: 0.10, session: 0.10}

KL(baseline || current) = high
# Interpretation: System experiencing more auth failures
```

**Parameter-level** (for auth failures):
```python
baseline = {root: 0.90, guest: 0.08, test: 0.02}
current  = {root: 0.20, guest: 0.05, admin: 0.75}

KL(baseline || current) = very high
# Interpretation: Brute force attack on admin account!
```

## Comparison with Ground Truth

| Aspect | Ground Truth | Hierarchical Approach |
|--------|-------------|----------------------|
| Templates for auth failures | 4 (E16, E17, E18, E19) | 2 log types, ~3 templates |
| Distinguishes user=root vs user=guest | Yes (separate templates) | Yes (parameter distribution) |
| Handles new username | Creates new template | Updates distribution |
| Total templates | 117 | ~20-30 log types, ~40-60 templates |
| Sparsity | High (avg 17 logs/template) | Low (avg 70+ logs/template) |
| KL divergence | Difficult (template set changes) | Easy (stable template set) |

## Example: Security Attack Detection

**Scenario**: SSH brute force attack trying multiple usernames

**Ground Truth Approach**:
```
Baseline templates: E17(guest), E18(root), E19(test)
Attack adds: E20(admin), E21(oracle), E22(mysql), E23(www), ...

Problem: Can't compute KL divergence (different template sets)
Workaround: Track "new template creation rate" - indirect signal
```

**Hierarchical Approach**:
```
Baseline:
  Log type: auth_failure (40% of all logs)
  P(username | auth_failure) = {root: 0.90, guest: 0.08, test: 0.02}

Attack:
  Log type: auth_failure (80% of all logs) ← KL divergence spike!
  P(username | auth_failure) = {root: 0.10, admin: 0.30, oracle: 0.25, ...} ← KL divergence spike!

Detection:
  - Template-level KL: 0.48 (moderate - more auth failures)
  - Parameter-level KL: 2.14 (high - username distribution changed!)
  - Combined signal: HIGH CONFIDENCE ATTACK
```

## Implementation Priorities

1. ✅ **Token Classification** - `token_classifier.rs` (done)
2. **Log Type Matching** - Fast lookup by STATIC signature
3. **Template Clustering** - Group by STATIC + PARAMETER types
4. **Parameter Extraction** - Extract values for distribution tracking
5. **Distribution Tracking** - Maintain P(param | log_type) over time windows
6. **KL Divergence** - Compute divergence at both levels

## Migration from Current System

**Current**: Regex patterns with value-specific clustering (117 templates)

**Step 1**: Run hierarchical classifier on dataset
```rust
for log in dataset {
    let (log_type, template, params) = hierarchical_classify(log);
    // Map to ground truth for evaluation
}
```

**Step 2**: Analyze coverage
- How many log types identified? (expect ~20-30)
- How many templates per log type? (expect 1-3)
- Parameter distributions (what values appear?)

**Step 3**: Compare with ground truth
- E16, E17, E18, E19 → Should map to 2 log types (with/without user field)
- Each log type tracks username distribution

**Step 4**: Deploy for production
- No ground truth needed
- Track distributions over time
- Compute KL divergence for anomaly detection
