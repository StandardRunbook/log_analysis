# AI Agent Instructions for Log Analysis Project

## Project Overview
This is a high-performance log analysis system written in Rust, focusing on zero-copy optimizations, batch processing, and parallel execution. The system can process up to 374,000 logs/second with 98.46% accuracy.

## Key Components
- `LogMatcher`: Core matching engine with zero-copy and parallel processing
- HTTP service (`log-analyzer-service`): RESTful API for log analysis
- Template management system with JSON-based template storage

## Development Workflows

### Building and Running
```bash
# Development build and run
cargo run --bin log-analyzer-service

# Production build
cargo run --release --bin log-analyzer-service
```

### Testing
- Run benchmarks: `cargo test --release -- --ignored benchmark`
- Benchmarks are in 5 modes (see `BENCHMARKS.md`)
- Test data is in `data/` directory

## Project-Specific Patterns

### Memory Management
- Use `SmallVec` for small collections (see `src/matcher.rs`)
- Thread-local scratch buffers for temporary allocations
- Avoid allocations in hot paths

### Concurrency
- Use `Rayon` for parallel processing of large batches (>1000 logs)
- Prefer `match_batch_parallel()` for large datasets
- Use thread-local storage for scratch buffers

### Performance Guidelines
- Batch size >10,000 for optimal parallel performance
- Use FxHashMap instead of standard HashMap
- Add `#[inline]` hints for hot path functions

## Key Files
- `src/matcher.rs`: Core matching engine
- `src/service.rs`: HTTP service implementation
- `cache/comprehensive_templates.json`: Default templates
- `OPTIMIZATIONS.md`: Detailed performance docs

## Integration Points
- HTTP API on port 3000 (configurable via PORT env var)
- JSON template files in `cache/` directory
- OpenTelemetry integration for monitoring

## Common Tasks
1. Adding new templates:
   ```rust
   matcher.add_template(LogTemplate {
       template_id: id,
       pattern: pattern,
       variables: vars,
       example: example
   });
   ```

2. Processing log batches:
   ```rust
   // For small batches (<1000)
   matcher.match_batch(&logs);
   
   // For large batches
   matcher.match_batch_parallel(&logs);
   ```