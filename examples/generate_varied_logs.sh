#!/bin/bash
# Generate a file of varied log shapes for OTel collector testing.
# Five distinct shapes — three match the seed templates (cpu_usage,
# memory_usage, disk_io), two are novel and will exercise the LLM
# cold path.
#
# Usage: ./generate_varied_logs.sh <out-file> [count]

set -e

OUT="${1:-/tmp/varied_logs.log}"
COUNT="${2:-100}"

> "$OUT"

USERS=("alice" "bob" "carol" "dave" "eve" "frank")
HOSTS=("api-01" "api-02" "db-primary" "db-replica" "cache-01")
COMPONENTS=("auth" "payments" "search" "notifications")

for i in $(seq 1 "$COUNT"); do
  case $((i % 5)) in
    0)
      printf 'cpu_usage: %d.%d%% - sample %d\n' \
        $((RANDOM % 100)) $((RANDOM % 100)) "$i" >> "$OUT"
      ;;
    1)
      printf 'memory_usage: %d.%dGB - probe %d\n' \
        $((RANDOM % 64)) $((RANDOM % 100)) "$i" >> "$OUT"
      ;;
    2)
      printf 'disk_io: %dMB/s - throughput %d\n' \
        $((RANDOM % 500)) "$i" >> "$OUT"
      ;;
    3)
      printf 'User %s logged in from 10.0.%d.%d\n' \
        "${USERS[$((RANDOM % ${#USERS[@]}))]}" \
        $((RANDOM % 256)) $((RANDOM % 256)) >> "$OUT"
      ;;
    4)
      printf 'ERROR: connection refused to %s while calling %s (attempt %d)\n' \
        "${HOSTS[$((RANDOM % ${#HOSTS[@]}))]}" \
        "${COMPONENTS[$((RANDOM % ${#COMPONENTS[@]}))]}" \
        $((RANDOM % 5)) >> "$OUT"
      ;;
  esac
done

echo "wrote $COUNT lines across 5 shape families to $OUT"
echo "---"
head -5 "$OUT"
echo "..."
