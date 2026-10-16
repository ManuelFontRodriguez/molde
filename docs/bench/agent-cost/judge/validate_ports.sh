#!/usr/bin/env bash
# Oracle validation, part 2 (DESIGN.md §6.5): every correct port, built by its own
# framework inside the judge container, must pass the oracle on T0, including the
# data-mutation test. Part 1 is oracle/selftest.py (every defect must fail).
#   docs/bench/agent-cost/judge/validate_ports.sh [port ...]
set -euo pipefail
BENCH="$(cd "$(dirname "$0")/../.." && pwd)"
PORTS="${*:-molde parity-hugo parity-zola parity-eleventy parity-astro parity-nextjs}"
docker run --rm -v "$BENCH":/bench:ro molde-bench-judge:latest bash -c "
set -e
for p in $PORTS; do
  python3 -c \"import sys; sys.path.insert(0,'/bench/agent-cost/runner'); from ports import stage; stage('\$p', '/bench', '/tmp/ws-\$p', '/bench/cross-framework/harness/data.json')\"
  printf '%-16s ' \"\$p\"
  (cd /tmp/ws-\$p && git init -q && git -c user.name=b -c user.email=b@l add -A && git -c user.name=b -c user.email=b@l commit -qm start)
  python3 /bench/agent-cost/judge/judge.py --port \$p --task T0 --workspace /tmp/ws-\$p --out /tmp/v-\$p
  python3 /bench/agent-cost/judge/summarize.py /tmp/v-\$p/verdict.json
done"
