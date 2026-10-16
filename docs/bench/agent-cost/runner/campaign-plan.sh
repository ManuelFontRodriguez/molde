#!/bin/sh
# The registered campaign (DESIGN.md §9 with the 16 Oct clarifications in LOG.md), seed 20261016.
# Writes results/campaign/plan.json once; refuses to overwrite it.
set -eu
cd "$(dirname "$0")/.."
MAIN=molde-check,molde-build,hugo,astro,nextjs,hugo+checker,astro+checker,nextjs+checker
exec python3 runner/campaign.py plan --set campaign --seed 20261016 \
  --spec "$MAIN|T0,T1,T2,T5,T6,T7,T8,T9,T10,T11,T12|P1|5" \
  --spec "$MAIN|T3,T4,T4b|P1|15" \
  --spec "molde-check,hugo,astro,nextjs|T1,T2,T3,T4,T5,T6|P2|5" \
  --spec "molde-mcp|T1,T2,T3,T4,T5,T6|P1|5" \
  --spec "parity-hugo,parity-zola,parity-eleventy,parity-astro,parity-nextjs|T1,T2,T3,T4,T5,T6|P1|5"
