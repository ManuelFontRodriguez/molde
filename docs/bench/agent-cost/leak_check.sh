#!/usr/bin/env bash
# Before anything in this folder is proposed for a public repository: no names of people,
# no private projects or services, no machine paths, no secrets. Exit 1 on any hit.
# Extend PRIVATE with the owner's own terms via BENCH_PRIVATE_TERMS (an extended regex,
# matched case-insensitively), which is kept outside the repository.
set -uo pipefail
cd "$(dirname "$0")"
# No /home/ pattern: transcripts legitimately show the container user's /home/bench and the
# molde reference's own /home/index.html example; the owner's machine paths are /Users/ and
# /private/ (macOS). Add a Linux home through BENCH_PRIVATE_TERMS if runs move there.
PRIVATE='/Users/|/private/(tmp|var)|~/Desktop|sk-ant-[A-Za-z0-9_-]{8,}'
[ -n "${BENCH_PRIVATE_TERMS:-}" ] && PRIVATE="$PRIVATE|$BENCH_PRIVATE_TERMS"
hits=$(grep -rIniE "$PRIVATE" . --exclude=leak_check.sh 2>/dev/null; \
       find results -name '*.tar.gz' -print0 2>/dev/null | while IFS= read -r -d '' t; do \
         tar -xzOf "$t" 2>/dev/null | grep -aIniE "$PRIVATE" | sed "s|^|$t: |"; done)
if [ -n "$hits" ]; then echo "$hits" | cut -c1-200; echo "LEAK CHECK FAILED"; exit 1; fi
echo "leak check: clean"
