#!/usr/bin/env bash
# Build the benchmark image from the committed tree only (git archive HEAD), so
# uncommitted files, target/ and anything private in the working copy never enter
# the image. Usage: docs/bench/agent-cost/docker/build.sh [--allow-dirty]
#
# --allow-dirty archives the working tree's tracked files as they are on disk
# (for development); a real run is always built from a clean commit.
set -euo pipefail
REPO="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
cd "$REPO"
CTX="$(mktemp -d)"
trap 'rm -rf "$CTX"' EXIT

if [[ "${1:-}" == "--allow-dirty" ]]; then
  git ls-files -z | tar --null -T - -cf - | tar -xf - -C "$CTX"
  # Untracked benchmark files under development, too (never anything outside this folder).
  git ls-files -z --others --exclude-standard docs/bench/agent-cost | tar --null -T - -cf - | tar -xf - -C "$CTX"
  REV="$(git rev-parse --short HEAD)-dirty"
else
  if [[ -n "$(git status --porcelain)" ]]; then
    echo "working tree not clean; commit first (or pass --allow-dirty for development)" >&2
    exit 1
  fi
  git archive HEAD | tar -xf - -C "$CTX"
  REV="$(git rev-parse --short HEAD)"
fi

# build.rs stamps the commit into `molde --version` from git; the archive has no
# .git, so the version reads as the package version alone. Record the commit here.
echo "$REV" > "$CTX/BENCH_SOURCE_REV"

for target in agent judge; do
  docker build -f docs/bench/agent-cost/docker/Dockerfile --target "$target" \
    -t "molde-bench-$target:$REV" -t "molde-bench-$target:latest" "$CTX"
  echo "built molde-bench-$target:$REV ($(docker image inspect -f '{{.Id}}' "molde-bench-$target:$REV"))"
done

# The egress proxy (no repository content, only its own folder).
docker build -t "molde-bench-egress:$REV" -t molde-bench-egress:latest "$REPO/docs/bench/agent-cost/docker/egress"
echo "built molde-bench-egress:$REV"
