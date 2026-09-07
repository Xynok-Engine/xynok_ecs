#!/usr/bin/env bash
# Runs both benchmarks and writes the report.
#
#   ./benches/scripts/bench.sh                 # everything
#   ./benches/scripts/bench.sh 1_archetype     # only the benchmarks whose id contains that
#
# The filter goes to both targets, so a filter that matches nothing in one of them just means that
# target runs no benchmarks, which is fine: the report says which scenarios have no timing yet.
#
# Criterion keeps the previous run on disk, so the second time this is run every benchmark also
# reports how it moved. Clearing `target/criterion` throws that history away.
set -euo pipefail

cd "$(dirname "$0")/../.."

filter="${1:-}"
if [[ -n "$filter" ]]; then
  cargo bench -p xynok_ecs_benches --bench query -- "$filter"
  cargo bench -p xynok_ecs_benches --bench parallel -- "$filter"
else
  cargo bench -p xynok_ecs_benches --bench query
  cargo bench -p xynok_ecs_benches --bench parallel
fi

cargo run --release -p xynok_ecs_benches --bin report

echo
echo "combined report: benches/output/report.html"
echo "criterion report: target/criterion/report/index.html"
