# Multi-LLM Configuration

This service supports querying multiple LLM providers in parallel and using consensus to ensure high-quality template generation.

## Configuration Methods

### 1. Environment Variables (Single LLM)

For simple single-LLM setup:

```bash
LLM_PROVIDER=ollama
LLM_MODEL=llama3
OLLAMA_ENDPOINT=http://localhost:11434

# For OpenAI
LLM_PROVIDER=openai
LLM_MODEL=gpt-4
LLM_API_KEY=sk-...
```

### 2. Configuration File (Multi-LLM with Consensus)

For advanced multi-LLM consensus:

```bash
LLM_CONFIG_FILE=./llm-config.json
```

Example configuration (`llm-config.json`):

```json
{
  "providers": [
    {
      "name": "ollama-llama3",
      "provider": "ollama",
      "model": "llama3",
      "api_key": null,
      "endpoint": "http://localhost:11434",
      "timeout_secs": 60
    },
    {
      "name": "openai-gpt4",
      "provider": "openai",
      "model": "gpt-4",
      "api_key": "sk-your-key",
      "endpoint": null,
      "timeout_secs": 60
    }
  ],
  "consensus_strategy": "majority",
  "min_agreement": 2
}
```

## Supported Providers

- **ollama**: Local Ollama instance
- **openai**: OpenAI API (GPT-3.5, GPT-4, etc.)
- **anthropic**: Anthropic API (Claude)

## Consensus Strategies

### `first_success`
- Try providers in order until one succeeds
- Fastest, no consensus required
- Good for single LLM or fallback scenarios

### `majority`
- Require >50% of providers to agree
- Requires at least 2 providers
- Good balance of speed and quality

### `unanimous`
- Require all providers to agree
- Highest quality, slowest
- Requires at least 2 providers

### `min_agreement`
- Require N providers to agree (specified by `min_agreement`)
- Flexible threshold
- Example: 2 out of 3 providers must agree

## How Consensus Works

1. All LLM providers are called **in parallel**
2. Each provider generates a regex pattern for the log line
3. Patterns are normalized (whitespace removed) and grouped
4. The consensus strategy determines if a group has enough votes
5. The most agreed-upon pattern is selected

### Example

With 3 providers and `majority` strategy:

```
Provider A: "error: (\d+) failed"
Provider B: "error: (\d+) failed"
Provider C: "error: (.*) failed"

Result: Pattern A wins (2/3 majority)
```

## Benefits of Multi-LLM Consensus

✅ **Higher accuracy**: Multiple models catch each other's mistakes
✅ **Reduced hallucination**: Consensus filters out outlier patterns
✅ **Provider redundancy**: If one provider is down, others continue
✅ **Cost optimization**: Mix expensive (GPT-4) with cheap (local Ollama) providers

## Performance Considerations

- **Parallel execution**: All LLMs are called simultaneously
- **Latency**: Overall latency = slowest provider's response time
- **Cost**: Multiply API costs by number of providers
- **Recommended**: Use `first_success` for high-volume, `majority` for quality-critical

## Example Configurations

### Production (High Quality)
```json
{
  "providers": [
    {"name": "gpt4", "provider": "openai", "model": "gpt-4", "api_key": "..."},
    {"name": "claude", "provider": "anthropic", "model": "claude-3-sonnet-20240229", "api_key": "..."}
  ],
  "consensus_strategy": "majority",
  "min_agreement": 2
}
```

### Development (Cost Effective)
```json
{
  "providers": [
    {"name": "local", "provider": "ollama", "model": "llama3", "endpoint": "http://localhost:11434"}
  ],
  "consensus_strategy": "first_success",
  "min_agreement": 1
}
```

### High Reliability (Redundancy)
```json
{
  "providers": [
    {"name": "primary", "provider": "openai", "model": "gpt-4", "api_key": "..."},
    {"name": "backup1", "provider": "anthropic", "model": "claude-3-sonnet-20240229", "api_key": "..."},
    {"name": "backup2", "provider": "ollama", "model": "llama3", "endpoint": "http://localhost:11434"}
  ],
  "consensus_strategy": "min_agreement",
  "min_agreement": 2
}
```
