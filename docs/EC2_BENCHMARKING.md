# EC2 Benchmarking Guide

This guide helps you benchmark the log analyzer on AWS EC2 instances and compare performance with local and KraftCloud deployments.

## Quick Start

### 1. Setup EC2 Instance

Launch an EC2 instance via AWS Console:

```bash
# Recommended instance types for benchmarking:
# - c5.large (2 vCPU, 4GB RAM) - General compute optimized
# - c5.xlarge (4 vCPU, 8GB RAM) - Better parallel performance
# - c6i.2xlarge (8 vCPU, 16GB RAM) - High performance testing
# - c7g.large (2 vCPU, 4GB RAM) - ARM Graviton (newest)

# AMI: Amazon Linux 2023 or Ubuntu 22.04
# Security Group: Allow SSH (22) from your IP
```

### 2. Deploy Binary

```bash
# From your local machine, run:
./scripts/deploy_to_ec2.sh ec2-user@<EC2_IP> ~/.ssh/your-key.pem

# This will:
# - Build the release binary
# - Copy binary to EC2
# - Copy cache files (templates)
# - Install dependencies
# - Set up benchmark scripts
```

### 3. Run Benchmarks

```bash
# Option A: Run comprehensive benchmarks with automatic comparison
./scripts/benchmark_ec2.sh ec2-user@<EC2_IP> ~/.ssh/your-key.pem

# Option B: Run manually on EC2
ssh -i ~/.ssh/your-key.pem ec2-user@<EC2_IP>
cd /home/ec2-user/log_analyzer
./run_benchmark_ec2.sh
```

## Detailed Instructions

### Setting Up Different Instance Types

#### T3 Instances (Burstable - Good for Testing)
```bash
# t3.medium: 2 vCPU, 4GB RAM (~$30/month)
# Good for: Initial testing, cost-sensitive workloads
# Limitation: CPU credits (burst performance)
```

#### C5 Instances (Compute Optimized - Recommended)
```bash
# c5.large: 2 vCPU, 4GB RAM (~$60/month)
# c5.xlarge: 4 vCPU, 8GB RAM (~$120/month)
# c5.2xlarge: 8 vCPU, 16GB RAM (~$240/month)
# Good for: Production-like benchmarks
# Best for: CPU-intensive pattern matching
```

#### C6i Instances (Latest Intel - Best Performance)
```bash
# c6i.large: 2 vCPU, 4GB RAM (~$65/month)
# c6i.xlarge: 4 vCPU, 8GB RAM (~$130/month)
# Good for: Latest Intel optimizations
# Best for: Maximum single-thread performance
```

#### C7g Instances (ARM Graviton - Best Price/Performance)
```bash
# c7g.large: 2 vCPU, 4GB RAM (~$50/month)
# c7g.xlarge: 4 vCPU, 8GB RAM (~$100/month)
# Good for: Cost optimization
# Note: Requires ARM binary compilation
```

### EC2 Instance Launch via CLI

```bash
# Install AWS CLI
aws configure

# Launch instance (update with your values)
aws ec2 run-instances \
  --image-id ami-0c55b159cbfafe1f0 \
  --instance-type c5.xlarge \
  --key-name your-key-name \
  --security-group-ids sg-xxxxxxxx \
  --subnet-id subnet-xxxxxxxx \
  --tag-specifications 'ResourceType=instance,Tags=[{Key=Name,Value=log-analyzer-benchmark}]'

# Get instance IP
aws ec2 describe-instances \
  --filters "Name=tag:Name,Values=log-analyzer-benchmark" \
  --query 'Reservations[0].Instances[0].PublicIpAddress' \
  --output text
```

### Manual Deployment Steps

If you prefer manual deployment:

```bash
# 1. Build binary locally
cargo build --release --bin log-ingest-service

# 2. Copy to EC2
scp -i ~/.ssh/your-key.pem \
  target/release/log-ingest-service \
  ec2-user@<EC2_IP>:/home/ec2-user/

# 3. Copy cache files
scp -i ~/.ssh/your-key.pem -r \
  cache/*.json \
  ec2-user@<EC2_IP>:/home/ec2-user/cache/

# 4. SSH and run
ssh -i ~/.ssh/your-key.pem ec2-user@<EC2_IP>
chmod +x log-ingest-service
export CLICKHOUSE_URL=http://localhost:8123
./log-ingest-service
```

### Running the Service

```bash
# Start the service
export CLICKHOUSE_URL=http://your-clickhouse:8123
export INGEST_PORT=3002
export RUST_LOG=info
./log-ingest-service

# Test endpoints
curl http://localhost:3002/health
curl http://localhost:3002/stats

# Send test logs
curl -X POST http://localhost:3002/logs/ingest \
  -H 'Content-Type: application/json' \
  -d '{"org":"test","message":"ERROR: connection timeout"}'
```

### Running Benchmarks

The benchmark script will:
1. Detect EC2 instance type and specs
2. Run throughput benchmarks
3. Test parallel processing
4. Compare with local results
5. Generate detailed reports

```bash
# From local machine (recommended)
./scripts/benchmark_ec2.sh ec2-user@<EC2_IP> ~/.ssh/your-key.pem

# Results saved to: benchmark_results/ec2_<timestamp>/
```

## Expected Performance

### By Instance Type

| Instance Type | vCPU | Memory | Expected Throughput | Cost/Month |
|--------------|------|--------|-------------------|------------|
| t3.medium | 2 | 4GB | 80-120K logs/sec | ~$30 |
| c5.large | 2 | 4GB | 100-150K logs/sec | ~$60 |
| c5.xlarge | 4 | 8GB | 200-300K logs/sec | ~$120 |
| c5.2xlarge | 8 | 16GB | 350-450K logs/sec | ~$240 |
| c6i.xlarge | 4 | 8GB | 220-320K logs/sec | ~$130 |
| c7g.xlarge | 4 | 8GB | 240-340K logs/sec | ~$100 |

### Performance Factors

**Single-threaded performance** (per log):
- 1-5 μs latency for pattern matching
- 200K+ logs/sec on modern CPU

**Parallel performance** (batch >1000 logs):
- Scales linearly with vCPU count
- 8 vCPU → ~8x single-thread throughput
- Limited by memory bandwidth above 16 cores

## Comparing with KraftCloud

### EC2 vs KraftCloud

| Metric | EC2 (c5.xlarge) | KraftCloud (Free Tier) |
|--------|----------------|----------------------|
| vCPUs | 4 | 1 per instance |
| Memory | 8GB | 4GB total |
| Throughput | 200-300K logs/sec | 40-60K logs/sec |
| Cold Start | 30-60s | <1s |
| Cost | ~$120/month | Free (2 instances) |
| Use Case | Production | Microservices/FaaS |

### When to Use Each

**EC2**:
- Production workloads
- Need >2 vCPUs
- Predictable performance
- Long-running services
- High throughput required

**KraftCloud**:
- Development/testing
- Microservices architecture
- Serverless functions
- Security-critical workloads
- Cost-sensitive projects

## Performance Optimization Tips

### For EC2

1. **Use compute-optimized instances** (C5/C6i family)
2. **Enable enhanced networking** (enabled by default on C5+)
3. **Use placement groups** for multi-instance deployments
4. **Monitor CPU credits** on T3 instances
5. **Use latest generation** instances (C6i/C7g over C5)

### For the Application

1. **Set RAYON_NUM_THREADS** to match vCPU count:
   ```bash
   export RAYON_NUM_THREADS=4  # For 4 vCPU instance
   ```

2. **Tune batch size** based on instance:
   ```bash
   # Small instances (2 vCPU): 500-1000 logs/batch
   # Medium instances (4 vCPU): 1000-2000 logs/batch
   # Large instances (8+ vCPU): 2000-5000 logs/batch
   ```

3. **Use local ClickHouse** for benchmarking to avoid network latency

## Cost Analysis

### Monthly Cost Comparison

```bash
# Free tier (limited time)
KraftCloud: $0 (2 instances, 1 vCPU each)

# Budget option
t3.medium: ~$30/month (burstable, good for testing)

# Production (small)
c5.large: ~$60/month (2 vCPU, consistent performance)

# Production (medium)
c5.xlarge: ~$120/month (4 vCPU, high throughput)

# High performance
c5.2xlarge: ~$240/month (8 vCPU, max throughput)
```

### Cost per Million Logs

Based on expected throughput:

| Instance | Throughput | Logs/Month* | Cost/M Logs |
|----------|-----------|-------------|-------------|
| t3.medium | 100K/sec | 259B | $0.12 |
| c5.large | 130K/sec | 336B | $0.18 |
| c5.xlarge | 250K/sec | 648B | $0.19 |
| KraftCloud | 50K/sec | 129B | $0 |

*Assumes 30 days uptime at peak throughput

## Troubleshooting

### Binary Won't Run

```bash
# Check if binary is for correct architecture
file log-ingest-service
# Should show: "ELF 64-bit LSB executable, x86-64"

# If you're using ARM (Graviton), rebuild for ARM:
# (On ARM EC2 instance)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
cargo build --release
```

### Low Performance

```bash
# Check CPU throttling
cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor
# Should be: "performance" not "powersave"

# Check available CPU
htop

# Check if using release build
./log-ingest-service --version
strings log-ingest-service | grep -i release
```

### Connection Issues

```bash
# Ensure security group allows traffic
aws ec2 describe-security-groups --group-ids sg-xxxxxxxx

# Test locally first
curl http://localhost:3002/health

# Check if service is running
ps aux | grep log-ingest
netstat -tlnp | grep 3002
```

## Cleanup

```bash
# Stop EC2 instance (to avoid charges)
aws ec2 stop-instances --instance-ids i-xxxxxxxx

# Terminate EC2 instance (permanent)
aws ec2 terminate-instances --instance-ids i-xxxxxxxx

# Or via console: EC2 → Instances → Select → Actions → Terminate
```

## Next Steps

1. Run benchmarks on multiple instance types
2. Compare results with local development machine
3. Test with KraftCloud deployment
4. Choose optimal instance type for production
5. Set up auto-scaling based on benchmarks
6. Implement monitoring and alerting

## Additional Resources

- [AWS EC2 Instance Types](https://aws.amazon.com/ec2/instance-types/)
- [AWS EC2 Pricing](https://aws.amazon.com/ec2/pricing/)
- [Rust Performance Tips](https://nnethercote.github.io/perf-book/)
- [Rayon Parallel Processing](https://github.com/rayon-rs/rayon)
