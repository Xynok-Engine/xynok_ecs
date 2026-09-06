#!/usr/bin/env bash
# Runs the benchmark and writes the report.
#
#   ./benches/scripts/bench.sh                 # everything
#   ./benches/scripts/bench.sh 1_archetype     # only the benchmarks whose id contains that
#
# Criterion keeps the previous run on disk, so the second time this is run every benchmark also
# reports how it moved. Clearing `target/criterion` throws that history away.
set -euo pipefail

cd "$(dirname "$0")/../.."

filter="${1:-}"
if [[ -n "$filter" ]]; then
  cargo bench -p xynok_ecs_benches --bench query -- "$filter"
else
  cargo bench -p xynok_ecs_benches --bench query
fi

cargo run --release -p xynok_ecs_benches --bin report

echo
echo "combined report: benches/output/report.html"
echo "criterion report: target/criterion/report/index.html"
