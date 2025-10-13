# Jensen-Shannon Divergence (JSD) Analysis Guide

## Overview

The log analyzer automatically calculates Jensen-Shannon Divergence (JSD) between the current log period and a baseline period (3 hours prior) to detect anomalies and changes in log patterns.

## What is JSD?

**Jensen-Shannon Divergence** is a method of measuring the similarity between two probability distributions. It's based on the Kullback-Leibler divergence but is:
- **Symmetric**: JSD(P||Q) = JSD(Q||P)
- **Bounded**: Always between 0 and ln(2) ≈ 0.693 (in nats) or 0 and 1 (in bits)
- **Smooth**: Handles zero probabilities better than KL divergence

### JSD Score Interpretation

| JSD Score | Meaning | What it Indicates |
|-----------|---------|-------------------|
| **0.0 - 0.01** | Nearly Identical | Log patterns are virtually unchanged |
| **0.01 - 0.05** | Minor Changes | Small shifts in log distribution, likely normal variation |
| **0.05 - 0.1** | Moderate Changes | Noticeable pattern changes, worth investigating |
| **0.1 - 0.3** | Significant Changes | Major shifts in log patterns, potential issues |
| **> 0.3** | Dramatic Changes | Very different distributions, likely incident or deployment |

## How It Works

### 1. Baseline Period Query

When you query logs for a time range, the system automatically:
- Calculates baseline period: 3 hours before your `start_time`
- Queries the same metric from that baseline period
- Builds a histogram of template IDs

**Example:**
```
User Request: 2025-01-15 10:00:00 to 10:30:00
Baseline Period: 2025-01-15 07:00:00 to 10:00:00
```

### 2. Histogram Generation

For both baseline and current periods, the system:
- Matches each log against templates in the radix trie
- Counts occurrences of each template ID
- Converts counts to probability distributions

**Example Histogram:**
```json
{
  "cpu_usage_1": 10,      // 10 logs matched this template
  "memory_usage_1": 3,    // 3 logs matched this template
  "disk_io_1": 2          // 2 logs matched this template
}

Total: 15 logs
Distribution:
{
  "cpu_usage_1": 0.667,      // 66.7%
  "memory_usage_1": 0.200,   // 20.0%
  "disk_io_1": 0.133         // 13.3%
}
```

### 3. JSD Calculation

The system calculates:

```
M = (P + Q) / 2                    // Mixture distribution
JSD(P||Q) = [KL(P||M) + KL(Q||M)] / 2
```

Where:
- **P** = Baseline probability distribution
- **Q** = Current probability distribution
- **M** = Average of P and Q
- **KL** = Kullback-Leibler divergence

### 4. Template Contribution Analysis

For each template, calculate its contribution to the overall JSD:

```rust
contribution = (kl_p_m + kl_q_m) / 2

where:
  kl_p_m = p * ln(p/m)  // If baseline has this template
  kl_q_m = q * ln(q/m)  // If current has this template
```

Templates are sorted by contribution (highest first).

## API Response Structure

```json
{
  "logs": [...],
  "count": 14,
  "matched_logs": 14,
  "unmatched_logs": 0,
  "new_templates_generated": 0,
  "jsd_analysis": {
    "jsd_score": 0.11098152389578031,
    "baseline_period": "2025-01-15 07:00:00 UTC to 2025-01-15 10:00:00 UTC",
    "current_period": "2025-01-15 10:00:00 UTC to 2025-01-15 10:30:00 UTC",
    "baseline_log_count": 2,
    "current_log_count": 14,
    "top_contributors": [
      {
        "template_id": "disk_io_1",
        "baseline_probability": 0.0,
        "current_probability": 0.14285714285714285,
        "contribution": 0.049510512847138956,
        "relative_change": 100.0
      },
      {
        "template_id": "memory_usage_1",
        "baseline_probability": 0.0,
        "current_probability": 0.14285714285714285,
        "contribution": 0.049510512847138956,
        "relative_change": 100.0
      },
      {
        "template_id": "cpu_usage_1",
        "baseline_probability": 1.0,
        "current_probability": 0.7142857142857143,
        "contribution": 0.011960498201502398,
        "relative_change": -28.57142857142857
      }
    ]
  }
}
```

### Field Explanations

| Field | Type | Description |
|-------|------|-------------|
| `jsd_score` | float | Overall JSD score (higher = more divergence) |
| `baseline_period` | string | Time range of baseline logs |
| `current_period` | string | Time range of current logs |
| `baseline_log_count` | int | Number of logs in baseline |
| `current_log_count` | int | Number of logs in current period |
| `top_contributors` | array | Templates sorted by JSD contribution |

### Top Contributor Fields

| Field | Type | Description |
|-------|------|-------------|
| `template_id` | string | Template identifier |
| `baseline_probability` | float | Probability in baseline (0.0-1.0) |
| `current_probability` | float | Probability in current period (0.0-1.0) |
| `contribution` | float | Contribution to overall JSD score |
| `relative_change` | float | Percentage change from baseline (%) |

## Use Cases

### 1. Anomaly Detection

**Scenario:** Sudden spike in error logs

```json
{
  "jsd_score": 0.45,
  "top_contributors": [
    {
      "template_id": "database_error_timeout",
      "baseline_probability": 0.01,
      "current_probability": 0.35,
      "contribution": 0.38,
      "relative_change": 3400.0
    }
  ]
}
```

**Interpretation:** Database timeout errors increased by 3400%, contributing heavily to the high JSD score. This indicates a likely database performance issue.

---

### 2. Deployment Detection

**Scenario:** New feature deployed with additional logging

```json
{
  "jsd_score": 0.23,
  "top_contributors": [
    {
      "template_id": "feature_x_activated",
      "baseline_probability": 0.0,
      "current_probability": 0.15,
      "contribution": 0.18,
      "relative_change": 100.0
    },
    {
      "template_id": "cache_hit",
      "baseline_probability": 0.20,
      "current_probability": 0.45,
      "contribution": 0.05,
      "relative_change": 125.0
    }
  ]
}
```

**Interpretation:** New log template appeared (feature X), and cache hit rate increased significantly. This suggests a successful deployment with improved caching.

---

### 3. Incident Recovery

**Scenario:** Error logs decreasing after fix

```json
{
  "jsd_score": 0.18,
  "top_contributors": [
    {
      "template_id": "api_timeout_error",
      "baseline_probability": 0.40,
      "current_probability": 0.05,
      "contribution": 0.15,
      "relative_change": -87.5
    }
  ]
}
```

**Interpretation:** API timeout errors dropped by 87.5%, indicating recovery from an incident.

---

### 4. Traffic Pattern Changes

**Scenario:** Different user behavior during peak hours

```json
{
  "jsd_score": 0.12,
  "top_contributors": [
    {
      "template_id": "user_login",
      "baseline_probability": 0.10,
      "current_probability": 0.25,
      "contribution": 0.06,
      "relative_change": 150.0
    },
    {
      "template_id": "api_search_query",
      "baseline_probability": 0.15,
      "current_probability": 0.30,
      "contribution": 0.04,
      "relative_change": 100.0
    }
  ]
}
```

**Interpretation:** More logins and search queries during this period, suggesting higher user engagement.

## Integration with Alerting

You can set up alerts based on JSD scores and template contributions:

### Alert Rule Examples

**Rule 1: High JSD Score**
```yaml
- name: high_jsd_anomaly
  condition: jsd_score > 0.3
  severity: warning
  message: "High log pattern divergence detected"
```

**Rule 2: New Error Template**
```yaml
- name: new_error_template
  condition: |
    top_contributor.template_id.contains("error") AND
    top_contributor.baseline_probability == 0 AND
    top_contributor.current_probability > 0.1
  severity: critical
  message: "New error pattern detected: {template_id}"
```

**Rule 3: Significant Template Change**
```yaml
- name: template_spike
  condition: |
    top_contributor.relative_change > 500 OR
    top_contributor.relative_change < -80
  severity: warning
  message: "Template frequency changed dramatically: {template_id}"
```

## Advanced Analysis

### Aggregating JSD Over Time

Track JSD scores over multiple time windows:

```python
# Example: Monitor JSD every 30 minutes for 24 hours
results = []
for i in range(48):
    end_time = start_time + timedelta(minutes=30*(i+1))
    start = start_time + timedelta(minutes=30*i)
    
    response = query_logs(
        metric="cpu_usage",
        start_time=start,
        end_time=end_time
    )
    
    results.append({
        "time": end_time,
        "jsd_score": response.jsd_analysis.jsd_score
    })

# Plot JSD over time to see trends
plot_jsd_timeline(results)
```

### Identifying Recurring Patterns

Compare JSD across similar time periods:

```python
# Compare today's 10am-11am with yesterday's 10am-11am
today = query_logs(
    metric="cpu_usage",
    start_time="2025-01-15T10:00:00Z",
    end_time="2025-01-15T11:00:00Z"
)

yesterday = query_logs(
    metric="cpu_usage",
    start_time="2025-01-14T10:00:00Z",
    end_time="2025-01-14T11:00:00Z"
)

# Compare distributions
jsd = calculate_jsd(
    today.histogram,
    yesterday.histogram
)
```

### Root Cause Analysis

Use template contributions to identify root causes:

```python
# Get top 3 contributors
top_3 = jsd_analysis.top_contributors[:3]

for contributor in top_3:
    if contributor.relative_change > 100:
        print(f"Template {contributor.template_id} is new or spiking")
        # Fetch example logs for this template
        examples = get_logs_by_template(contributor.template_id)
        print(f"Examples: {examples[:5]}")
```

## Mathematical Details

### KL Divergence Formula

```
KL(P||Q) = Σ p(i) * ln(p(i) / q(i))
```

where the sum is over all possible values i.

### JSD Formula (Full)

```
JSD(P||Q) = H(M) - [H(P) + H(Q)]/2

where:
  M = (P + Q) / 2
  H(X) = -Σ x(i) * ln(x(i))  // Shannon entropy
```

Alternatively:
```
JSD(P||Q) = [KL(P||M) + KL(Q||M)] / 2
```

Both formulas are equivalent.

### Handling Zero Probabilities

When a template exists in one distribution but not the other:
- Add small epsilon (1e-10) to avoid log(0)
- Or handle explicitly: if p=0, then p*ln(p/m) = 0

Our implementation uses the epsilon approach.

## Configuration

### Adjusting Baseline Window

Default is 3 hours, but you can modify in `main.rs`:

```rust
// Change this line:
let baseline_duration = Duration::hours(3);

// To different values:
let baseline_duration = Duration::hours(6);   // 6 hours
let baseline_duration = Duration::hours(1);   // 1 hour
let baseline_duration = Duration::days(1);    // 24 hours
```

### Filtering Low-Frequency Templates

Optionally filter out rare templates before JSD calculation:

```rust
fn filter_low_frequency_templates(histogram: &mut Histogram, min_count: usize) {
    histogram.counts.retain(|_, &mut count| count >= min_count);
    histogram.total = histogram.counts.values().sum();
}
```

## Performance Considerations

- **Baseline Query**: Requires fetching historical logs (can be cached)
- **Histogram Building**: O(n) where n = number of logs
- **JSD Calculation**: O(k) where k = unique templates
- **Memory**: Two histograms stored (baseline + current)

### Optimization Tips

1. **Cache baseline histograms** for overlapping time ranges
2. **Pre-aggregate histograms** at log ingestion time
3. **Sample large log volumes** if needed
4. **Store histograms** in time-series database for fast access

## Troubleshooting

### JSD is Always High

**Possible Causes:**
- Baseline period has very few logs
- System just started (no historical data)
- Templates changing frequently (LLM generating too many)

**Solutions:**
- Ensure baseline period has sufficient logs (>100)
- Wait for template stabilization
- Review template generation rules

### JSD is Always Low

**Possible Causes:**
- Not enough template diversity
- Templates too generic (matching everything)
- Logs too uniform

**Solutions:**
- Review template specificity
- Check if templates are properly differentiating log types
- Ensure logs contain varying information

### Null JSD Analysis

**Causes:**
- Insufficient data in baseline or current period
- No logs matched any templates

**Solutions:**
- Check metric name is correct
- Verify time ranges have logs
- Review template matching logic
