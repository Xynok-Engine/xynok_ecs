#!/usr/bin/env bash
#
# Runs the parallel benchmark once per thread count, because bevy's `ComputeTaskPool` is a global
# `OnceLock` and can only be sized once per process.
#
# The thread count is part of every benchmark id, so criterion files each run under its own name and
# the report ends up holding the whole curve instead of only the last point.
#
#   ./benches/scripts/parallel_scaling.sh              # 1 2 4 8 and cores-1
#   ./benches/scripts/parallel_scaling.sh 1 2 3 4      # exactly these
#
# Afterwards: open target/criterion/report/index.html
set -euo pipefail

cd "$(dirname "$0")/../.."

if [ "$#" -gt 0 ]; then
  THREAD_COUNTS=("$@")
else
  # Physical cores would be the better default, but there is no portable way to ask for them, and
  # over-counting on a hyper-threaded machine at least shows where the curve flattens.
  if command -v nproc >/dev/null 2>&1; then
    CORES=$(nproc)
  elif command -v sysctl >/dev/null 2>&1; then
    CORES=$(sysctl -n hw.logicalcpu)
  else
    CORES=4
  fi

  THREAD_COUNTS=(1 2 4 8)
  # One worker per core minus the calling thread, which is what a real engine ships with. Only
  # added if the loop above missed it.
  DEFAULT=$((CORES - 1))
  case " ${THREAD_COUNTS[*]} " in
    *" ${DEFAULT} "*) ;;
    *) THREAD_COUNTS+=("${DEFAULT}") ;;
  esac
fi

echo "sweeping worker thread counts: ${THREAD_COUNTS[*]}"
echo "(each count is one extra thread in practice: the calling thread runs jobs too)"
echo

for threads in "${THREAD_COUNTS[@]}"; do
  echo "==> XYNOK_BENCH_THREADS=${threads}"
  XYNOK_BENCH_THREADS="${threads}" cargo bench -p xynok_ecs_benches --bench parallel
  echo
done

echo "done. the full curve is in target/criterion/report/index.html"
