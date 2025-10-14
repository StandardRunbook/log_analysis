# Configuration Guide

The Log Analyzer service is configured using environment variables. This allows you to easily customize the service for different environments (development, staging, production).

## Required Environment Variables

### ClickHouse Configuration
- **`CLICKHOUSE_ENDPOINT`** (required)
  - The endpoint URL for your ClickHouse server
  - Example: `http://localhost:8123`
  - Example: `https://clickhouse.example.com:8443`

### LLM Service Configuration
- **`LLM_ENDPOINT`** (required)
  - The endpoint URL for your LLM service
  - Example: `https://api.openai.com/v1`
  - Example: `https://api.anthropic.com/v1`
  - Example: `http://localhost:8080` (for self-hosted LLM)

- **`LLM_API_SECRET`** (required)
  - API key/secret for authenticating with the LLM service
  - This is passed as a Bearer token in the Authorization header
  - Example: `sk-proj-abc123...`

## Optional Environment Variables

### Server Configuration
- **`SERVER_HOST`** (optional, default: `127.0.0.1`)
  - The host address to bind the server to
  - Use `0.0.0.0` to allow external connections
  - Example: `0.0.0.0`

- **`SERVER_PORT`** (optional, default: `3001`)
  - The port number to bind the server to
  - Example: `3001`

## Setup Instructions

### 1. Copy the example environment file

```bash
cp .env.example .env
```

### 2. Edit the `.env` file with your configuration

```bash
# Open in your preferred editor
nano .env
# or
vim .env
```

### 3. Set your actual values

```bash
# Server Configuration
SERVER_HOST=127.0.0.1
SERVER_PORT=3001

# ClickHouse Configuration
CLICKHOUSE_ENDPOINT=http://your-clickhouse-server:8123

# LLM Service Configuration
LLM_ENDPOINT=https://api.openai.com/v1
LLM_API_SECRET=sk-proj-your-actual-api-key
```

### 4. Load environment variables and run

#### Option A: Using a .env file with a tool

If you have `direnv` or similar:

```bash
direnv allow
cargo run
```

#### Option B: Export manually

```bash
export CLICKHOUSE_ENDPOINT="http://localhost:8123"
export LLM_ENDPOINT="https://api.openai.com/v1"
export LLM_API_SECRET="sk-proj-your-key"
cargo run
```

#### Option C: Inline with cargo run

```bash
CLICKHOUSE_ENDPOINT="http://localhost:8123" \
LLM_ENDPOINT="https://api.openai.com/v1" \
LLM_API_SECRET="sk-proj-your-key" \
cargo run
```

## Docker Configuration

If running in Docker, pass environment variables using `-e` flags or an env file:

```bash
docker run \
  -e CLICKHOUSE_ENDPOINT="http://clickhouse:8123" \
  -e LLM_ENDPOINT="https://api.openai.com/v1" \
  -e LLM_API_SECRET="sk-proj-your-key" \
  -e SERVER_HOST="0.0.0.0" \
  -e SERVER_PORT="3001" \
  -p 3001:3001 \
  log-analyzer
```

Or use an env file:

```bash
docker run --env-file .env -p 3001:3001 log-analyzer
```

## Kubernetes Configuration

Create a Secret for sensitive values:

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: log-analyzer-secrets
type: Opaque
stringData:
  llm-api-secret: "sk-proj-your-key"
```

Create a ConfigMap for non-sensitive values:

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: log-analyzer-config
data:
  CLICKHOUSE_ENDPOINT: "http://clickhouse-service:8123"
  LLM_ENDPOINT: "https://api.openai.com/v1"
  SERVER_HOST: "0.0.0.0"
  SERVER_PORT: "3001"
```

Reference in your Deployment:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: log-analyzer
spec:
  template:
    spec:
      containers:
      - name: log-analyzer
        image: log-analyzer:latest
        envFrom:
        - configMapRef:
            name: log-analyzer-config
        env:
        - name: LLM_API_SECRET
          valueFrom:
            secretKeyRef:
              name: log-analyzer-secrets
              key: llm-api-secret
        ports:
        - containerPort: 3001
```

## Validation

When the service starts, it will log the configuration (with the API secret partially masked):

```
2025-10-13T22:00:00.000000Z  INFO log_analyzer: 📋 Configuration:
2025-10-13T22:00:00.000000Z  INFO log_analyzer:    Server: 127.0.0.1:3001
2025-10-13T22:00:00.000000Z  INFO log_analyzer:    ClickHouse Endpoint: http://localhost:8123
2025-10-13T22:00:00.000000Z  INFO log_analyzer:    LLM Endpoint: https://api.openai.com/v1
2025-10-13T22:00:00.000000Z  INFO log_analyzer:    LLM API Secret: sk-p***
```

If any required environment variables are missing, the service will exit with an error message explaining what needs to be set.

## Security Best Practices

1. **Never commit `.env` files to version control**
   - Add `.env` to your `.gitignore`
   - Only commit `.env.example` with placeholder values

2. **Use secrets management in production**
   - AWS Secrets Manager
   - HashiCorp Vault
   - Kubernetes Secrets
   - Azure Key Vault

3. **Rotate API keys regularly**
   - Change `LLM_API_SECRET` periodically
   - Update in all environments after rotation

4. **Limit network access**
   - Use firewall rules to restrict access to ClickHouse
   - Use HTTPS for all external API calls

5. **Use read-only credentials where possible**
   - ClickHouse user should have minimal required permissions
   - LLM API key should have minimal required scopes
