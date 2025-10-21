# Template Strategy for KL Divergence Analysis

## Problem Statement

For KL divergence to detect log distribution shifts, we need to decide:
- Should `user=root` vs `user=guest` be separate templates or one template with parameters?
- How do we balance granularity vs generalization?

## Current Ground Truth Approach (Value-Specific)

**Philosophy**: Each unique combination of literal values = separate template

```
E18: auth failure; ... rhost=<*> user=root    (351 logs)
E17: auth failure; ... rhost=<*> user=guest   (17 logs)
E19: auth failure; ... rhost=<*> user=test    (4 logs)
E16: auth failure; ... rhost=<*>              (117 logs, no user field)
```

**Stats**: 117 templates for 1999 logs (5.8% diversity)

**Issues for KL Divergence**:
1. ❌ Template explosion - can't distinguish "increase in auth failures" from "increase in user=root"
2. ❌ Sparse distributions - many templates with only 1-4 occurrences
3. ❌ Can't generalize to new values (first time seeing `user=admin` creates new template)
4. ✅ Very precise - can detect specific username patterns

## Recommended Approach: Hierarchical Template + Parameters

### Level 1: Semantic Templates (Structure)

Create templates that capture **log structure**, not literal values:

```
T1: authentication failure; logname=<empty> uid=<uid> euid=<euid> tty=<tty> ruser=<empty> rhost=<rhost>
T2: authentication failure; logname=<empty> uid=<uid> euid=<euid> tty=<tty> ruser=<empty> rhost=<rhost> user=<username>
T3: session opened for user <username> by (uid=<uid>)
T4: session closed for user <username>
T5: connection from <ip> (<hostname>) at <timestamp>
```

**Result**: ~20-30 semantic templates instead of 117

### Level 2: Parameter Distributions (Values)

For each template, track parameter value distributions:

```json
{
  "template_id": "T2",
  "pattern": "auth failure ... rhost=<rhost> user=<username>",
  "parameters": {
    "username": {
      "root": 0.936,    // 351/375
      "guest": 0.045,   // 17/375
      "test": 0.011,    // 4/375
      "admin": 0.008    // 3/375
    },
    "tty": {
      "NODEVssh": 1.0
    },
    "rhost": {
      "220-135-151-1.hinet-ip.hinet.net": 0.12,
      "218.188.2.4": 0.08,
      // ... many more IPs
    }
  }
}
```

### KL Divergence Calculation

You can now compute KL divergence at **two levels**:

#### 1. Template-Level KL Divergence
Detects **structural changes** in log patterns:

```
P_baseline(template) = {T1: 0.40, T2: 0.30, T3: 0.20, T4: 0.10}
P_current(template)  = {T1: 0.10, T2: 0.60, T3: 0.20, T4: 0.10}
                              ↑ FTP connections dropped
                                   ↑ Auth failures increased

KL(P_baseline || P_current) = 0.48  ← High divergence!
```

#### 2. Parameter-Level KL Divergence (per template)
Detects **behavioral changes** within same log type:

```
P_baseline(username | auth_failure) = {root: 0.90, guest: 0.08, test: 0.02}
P_current(username | auth_failure)  = {root: 0.20, guest: 0.05, admin: 0.75}
                                            ↓ root dropped            ↑ admin attacks!

KL(P_baseline || P_current) = 1.86  ← Very high divergence!
```

## Implementation Strategy

### Phase 1: Semantic Template Generation (LLM)

**Prompt Philosophy**: Generate templates that capture **structure**, not specific values

```
CRITICAL: Your goal is to create GENERAL templates, not value-specific patterns.

RULES:
1. If a field value could plausibly change, make it a variable
2. Only keep literal text that indicates the LOG TYPE:
   - Keywords: "authentication", "failure", "opened", "closed"
   - Field names: "uid=", "user=", "rhost="
3. Replace all actual values with variables:
   - "user=root" → "user=(<username>)" NOT "user=root"
   - "rhost=218.188.2.4" → "rhost=(<rhost>)" NOT separate patterns for IP vs hostname

EXAMPLE (showing what NOT to do):
❌ BAD: Create 3 separate templates:
   - "auth failure ... user=root"
   - "auth failure ... user=guest"
   - "auth failure ... user=test"

✅ GOOD: Create 1 template with parameter:
   - "auth failure ... user=(<username>)"
```

### Phase 2: Parameter Extraction

After matching a log to a template, extract parameter values:

```rust
pub struct TemplateMatch {
    pub template_id: u64,
    pub parameters: HashMap<String, String>,  // e.g., {"username": "root", "rhost": "218.188.2.4"}
}
```

### Phase 3: Distribution Tracking

Maintain distributions over time windows:

```rust
pub struct TemplateDistribution {
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,

    // Template-level distribution
    pub template_counts: HashMap<u64, usize>,

    // Parameter-level distributions (per template)
    pub parameter_distributions: HashMap<u64, HashMap<String, HashMap<String, usize>>>,
    // template_id -> parameter_name -> value -> count
}
```

### Phase 4: KL Divergence Computation

```rust
pub fn compute_kl_divergence(
    baseline: &TemplateDistribution,
    current: &TemplateDistribution,
    level: DivergenceLevel,
) -> f64 {
    match level {
        DivergenceLevel::Template => {
            // Compare P(template) distributions
            kl_divergence(&baseline.template_probs(), &current.template_probs())
        }
        DivergenceLevel::Parameter(template_id, param_name) => {
            // Compare P(value | template, parameter) distributions
            let baseline_params = baseline.parameter_probs(template_id, param_name);
            let current_params = current.parameter_probs(template_id, param_name);
            kl_divergence(&baseline_params, &current_params)
        }
    }
}
```

## Benefits of This Approach

1. ✅ **Detects structural shifts**: "Suddenly more FTP connection logs"
2. ✅ **Detects behavioral shifts**: "Auth failures targeting different usernames"
3. ✅ **Handles novel values**: New username `admin` doesn't create new template, just updates parameter distribution
4. ✅ **Avoids sparsity**: Fewer templates means better statistics
5. ✅ **Hierarchical analysis**: Can drill down from template-level to parameter-level anomalies

## Comparison

| Metric | Ground Truth (Value-Specific) | Recommended (Semantic + Params) |
|--------|-------------------------------|----------------------------------|
| # Templates | 117 | ~20-30 |
| Avg logs/template | 17 | ~70-100 |
| Handles new values | ❌ Creates new template | ✅ Updates param distribution |
| Template-level KL | ❌ Noisy (too granular) | ✅ Clean signal |
| Parameter-level KL | N/A (baked into templates) | ✅ Tracked separately |
| Anomaly detection | Value-specific only | Structural + Behavioral |

## Example: Security Attack Detection

**Scenario**: Brute force SSH attack trying multiple usernames

**Ground Truth Approach** (Value-Specific):
```
Baseline: E18(user=root): 90%, E17(user=guest): 8%, E19(user=test): 2%
Attack:   E18(user=root): 10%, E17(user=guest): 10%, E20(user=admin): 40%,
          E21(user=oracle): 30%, E22(user=mysql): 10%

Result: 3 NEW templates created (E20, E21, E22)
Issue: Hard to compute KL divergence (different template sets)
```

**Semantic + Params Approach**:
```
Baseline: T2(auth_failure_with_user) param distribution:
          {root: 0.90, guest: 0.08, test: 0.02}

Attack:   T2(auth_failure_with_user) param distribution:
          {root: 0.10, guest: 0.10, admin: 0.40, oracle: 0.30, mysql: 0.10}

Result: KL(baseline || attack) = 2.14 (HIGH!)
Issue: None - clear signal of attack
```

## Recommendation for Your System

**Use the Semantic + Parameters approach** because:
1. You care about distribution shifts (KL divergence)
2. You need to handle novel parameter values gracefully
3. You want both structural and behavioral anomaly detection
4. The ground truth approach is designed for clustering, not anomaly detection

The value-specific ground truth (E18/E17/E19) is useful for **log parsing benchmarks** but not optimal for **production anomaly detection**.
