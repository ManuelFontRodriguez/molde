#!/bin/sh
# Round 2A (ROUND-2A.md): 5 arms x 9 tasks x 5 repetitions = 225 runs, seed 20261020.
# Writes results/round2a/plan.json once; refuses to overwrite it.
set -eu
cd "$(dirname "$0")/.."
exec python3 runner/campaign.py plan --set round2a --seed 20261020 \
  --spec "molde-check,molde-check-guide,hugo,astro,nextjs|T0,T1,T2,T3,T4,T4b,T5,T6,T10|P1|5"
