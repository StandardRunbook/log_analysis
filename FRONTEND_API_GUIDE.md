# Log Analyzer API - Frontend Integration Guide

## Quick Reference

- **Base URL**: `http://127.0.0.1:3001`
- **Endpoint**: `POST /query_logs`
- **Content-Type**: `application/json`
- **CORS**: ✅ Enabled (allows requests from any origin)

---

## Request Format

### Schema

```typescript
interface LogQueryRequest {
  metric_name: string;      // Metric to query (e.g., "cpu_usage")
  graph_name: string;       // Graph/panel name (e.g., "System Performance")
  start_time: string;       // ISO 8601 UTC timestamp
  end_time: string;         // ISO 8601 UTC timestamp
}
```

### Example Requests

#### Example 1: Query CPU Usage
```json
{
  "metric_name": "cpu_usage",
  "graph_name": "CPU Performance",
  "start_time": "2025-01-15T10:00:00Z",
  "end_time": "2025-01-15T10:30:00Z"
}
```

#### Example 2: Query Memory Usage
```json
{
  "metric_name": "memory_usage",
  "graph_name": "Memory Monitor",
  "start_time": "2025-01-15T14:00:00Z",
  "end_time": "2025-01-15T15:00:00Z"
}
```

#### Example 3: Query Disk I/O
```json
{
  "metric_name": "disk_io",
  "graph_name": "Disk Performance",
  "start_time": "2025-01-15T08:00:00Z",
  "end_time": "2025-01-15T10:00:00Z"
}
```

---

## Response Format

### Schema

```typescript
interface LogQueryResponse {
  log_groups: LogGroup[];
}

interface LogGroup {
  representative_logs: string[];  // Up to 3 sample log lines
  relative_change: number;        // Percentage change from baseline (-100 to +∞)
}
```

### Example Response (Success - 200 OK)

```json
{
  "log_groups": [
    {
      "representative_logs": [
        "disk_io: 250MB/s - Disk activity moderate",
        "disk_io: 250MB/s - Disk activity moderate"
      ],
      "relative_change": 100.0
    },
    {
      "representative_logs": [
        "memory_usage: 2.5GB - Memory consumption stable",
        "memory_usage: 2.5GB - Memory consumption stable"
      ],
      "relative_change": 100.0
    },
    {
      "representative_logs": [
        "cpu_usage: 45.2% - Server load normal",
        "cpu_usage: 67.8% - Server load increased",
        "cpu_usage: 89.3% - High server load detected"
      ],
      "relative_change": -28.571428571428573
    }
  ]
}
```

---

## Error Responses

### 400 Bad Request - Invalid Time Range
```json
{
  "error": "start_time must be before end_time"
}
```

### 400 Bad Request - Insufficient Data
```json
{
  "error": "Insufficient data for JSD calculation"
}
```

### 500 Internal Server Error
```json
{
  "error": "Failed to query metadata service: [details]"
}
```

---

## Understanding the Response

### Log Groups Ordering
- **Log groups are sorted by importance** (highest contribution to anomaly score first)
- The **first group** represents the most significant change from baseline
- Display groups in the order received

### Relative Change Field

The `relative_change` value indicates how much this log pattern changed compared to the baseline period (3 hours prior to query start):

| Value | Meaning | UI Suggestion |
|-------|---------|---------------|
| `100.0` | **Doubled** (100% increase) - New or significantly elevated | 🔴 Red badge "↑100%" |
| `50.0` | **50% increase** - Moderate increase in frequency | 🟠 Orange badge "↑50%" |
| `0.0` | **No change** - Same frequency as baseline | ⚪ Gray badge "0%" |
| `-28.57` | **28.57% decrease** - Less frequent than normal | 🟢 Green badge "↓29%" |
| `-100.0` | **Disappeared** - Was present in baseline but not now | 🔵 Blue badge "↓100%" |

### Positive vs Negative Changes

- **Positive** (`> 0`): Pattern increased in frequency
  - Could indicate: new errors, increased warnings, anomalies
  - UI: Red/Orange for alerts
  
- **Negative** (`< 0`): Pattern decreased in frequency
  - Could indicate: improvements, error reduction, recovery
  - UI: Green/Blue for positive changes

---

## Frontend Display Recommendations

### 1. Log Groups Container
```html
<div class="log-groups">
  <!-- Iterate through log_groups array -->
</div>
```

### 2. Individual Log Group Card
```html
<div class="log-group-card">
  <!-- Badge with relative_change -->
  <div class="change-badge" style="background: ${getColor(relative_change)}">
    ${formatChange(relative_change)}
  </div>
  
  <!-- Representative logs -->
  <div class="log-entries">
    <code>${log_line_1}</code>
    <code>${log_line_2}</code>
    <code>${log_line_3}</code>
  </div>
</div>
```

### 3. Color Coding Function (JavaScript)

```javascript
function getChangeColor(relativeChange) {
  if (relativeChange > 50) return '#DC2626';     // Red - Critical increase
  if (relativeChange > 10) return '#F59E0B';     // Orange - Warning
  if (relativeChange > -10) return '#6B7280';    // Gray - Neutral
  return '#10B981';                               // Green - Improvement
}

function formatChange(relativeChange) {
  const arrow = relativeChange > 0 ? '↑' : '↓';
  return `${arrow}${Math.abs(relativeChange).toFixed(1)}%`;
}
```

### 4. React Component Example

```jsx
function LogGroupCard({ logGroup }) {
  const { representative_logs, relative_change } = logGroup;
  const color = getChangeColor(relative_change);
  const formatted = formatChange(relative_change);
  
  return (
    <div className="log-group-card">
      <div className="badge" style={{ backgroundColor: color }}>
        {formatted}
      </div>
      <div className="logs">
        {representative_logs.map((log, idx) => (
          <code key={idx} className="log-line">{log}</code>
        ))}
      </div>
    </div>
  );
}

function LogAnalyzer({ metricName, startTime, endTime }) {
  const [data, setData] = useState(null);
  
  useEffect(() => {
    fetch('http://127.0.0.1:3001/query_logs', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        metric_name: metricName,
        start_time: startTime,
        end_time: endTime,
      }),
    })
    .then(res => res.json())
    .then(setData);
  }, [metricName, startTime, endTime]);
  
  if (!data) return <div>Loading...</div>;
  
  return (
    <div className="log-analyzer">
      <h2>Log Analysis: {metricName}</h2>
      {data.log_groups.map((group, idx) => (
        <LogGroupCard key={idx} logGroup={group} />
      ))}
    </div>
  );
}
```

---

## Grafana Integration

### Datasource Configuration

1. **Type**: JSON API or SimpleJson
2. **URL**: `http://127.0.0.1:3001`
3. **Method**: POST
4. **Custom Headers** (if auth is added):
   - Header: `Authorization`
   - Value: `Bearer YOUR_TOKEN`

### Query Template for Grafana

```json
{
  "metric_name": "${metric}",
  "start_time": "${__from:date:iso}",
  "end_time": "${__to:date:iso}"
}
```

**Variables to create:**
- `$metric` - Dropdown with values: `cpu_usage`, `memory_usage`, `disk_io`
- `$__from` - Built-in Grafana time range (start)
- `$__to` - Built-in Grafana time range (end)

---

## Testing with curl

### Basic Test
```bash
curl -X POST http://127.0.0.1:3001/query_logs \
  -H "Content-Type: application/json" \
  -d '{
    "metric_name": "cpu_usage",
    "graph_name": "CPU Performance",
    "start_time": "2025-01-15T10:00:00Z",
    "end_time": "2025-01-15T10:30:00Z"
  }'
```

### With Pretty Output (requires `jq`)
```bash
curl -X POST http://127.0.0.1:3001/query_logs \
  -H "Content-Type: application/json" \
  -d '{
    "metric_name": "cpu_usage",
    "graph_name": "CPU Performance",
    "start_time": "2025-01-15T10:00:00Z",
    "end_time": "2025-01-15T10:30:00Z"
  }' | jq .
```

### Test with JavaScript (fetch)
```javascript
fetch('http://127.0.0.1:3001/query_logs', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    metric_name: 'cpu_usage',
    graph_name: 'CPU Performance',
    start_time: '2025-01-15T10:00:00Z',
    end_time: '2025-01-15T10:30:00Z',
  }),
})
.then(response => response.json())
.then(data => console.log(data))
.catch(error => console.error('Error:', error));
```

---

## Available Metrics (Current Mock Data)

- `cpu_usage` - CPU utilization metrics
- `memory_usage` - Memory consumption metrics
- `disk_io` - Disk I/O performance metrics

---

## Important Notes

### Timezone Handling
- All timestamps are in **UTC**
- Convert to user's local timezone in the UI if needed
- JavaScript example: `new Date('2025-01-15T10:00:00Z').toLocaleString()`

### Baseline Period
- The API automatically queries a **3-hour baseline period** before the requested `start_time`
- The `relative_change` compares the current period to this baseline
- No need to specify baseline in the request

### Response Size
- Each log group contains **up to 3 representative logs**
- Typically returns **3-10 log groups** (most significant changes)
- Groups are already sorted by importance

### Error Handling
- Always check HTTP status code
- Parse `error` field from response body on 400/500 errors
- Display user-friendly error messages

---

## CSS Styling Suggestions

```css
.log-group-card {
  border: 1px solid #e5e7eb;
  border-radius: 8px;
  padding: 16px;
  margin-bottom: 12px;
  background: white;
}

.change-badge {
  display: inline-block;
  padding: 4px 12px;
  border-radius: 12px;
  font-weight: 600;
  font-size: 14px;
  color: white;
  margin-bottom: 12px;
}

.log-entries {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.log-line {
  font-family: 'Monaco', 'Courier New', monospace;
  font-size: 13px;
  padding: 8px 12px;
  background: #f9fafb;
  border-left: 3px solid #d1d5db;
  border-radius: 4px;
  overflow-x: auto;
  white-space: pre-wrap;
  word-break: break-all;
}
```

---

## Questions or Issues?

If you encounter any issues or have questions about the API:

1. Check the server logs for detailed request/response information
2. Verify the service is running: `http://127.0.0.1:3001`
3. Test with curl first to isolate frontend vs backend issues
4. Ensure timestamps are in ISO 8601 format with 'Z' suffix (UTC)

**Server logs will show:**
- Incoming requests with headers
- Query parameters (metric, time range)
- Processing details
- Response status

Start the server to see these logs:
```bash
cd log_analysis && cargo run
```
