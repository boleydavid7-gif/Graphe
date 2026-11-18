#!/usr/bin/env bash
# Create a clean working copy of a REVIEWED Graphe commit. Does NOT publish it.
# Usage: bash scripts/export-public-snapshot.sh <approved-commit-sha> <new-empty-directory>
set -euo pipefail
if [[ $# -ne 2 ]]; then
  echo "Usage: $0 <approved-commit-sha> <new-empty-directory>" >&2
  exit 2
fi
ref="$1"
destination="$2"
if ! git rev-parse --verify "$ref^{commit}" >/dev/null 2>&1; then
  echo "That reference is not a local commit." >&2
  exit 1
fi
if [[ -e "$destination" ]]; then
  echo "Destination must not exist. Pick a new directory." >&2
  exit 1
fi
mkdir -p "$destination"
git archive --format=tar "$ref" | tar -xf - -C "$destination"
echo "Snapshot created at: $destination"
echo "No original .git history was copied."
echo "DO NOT PUBLISH YET. Review all files/assets and run local checks:"
echo "  cd '$destination'"
echo "  node scripts/check-public-release.mjs"
echo "A fresh git repository and PRIVATE GitHub remote must be created only after clearance."
