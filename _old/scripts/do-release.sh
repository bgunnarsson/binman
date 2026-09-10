#!/usr/bin/env bash
set -euo pipefail

# Usage: ./scripts/do-release.sh 1.2.0
VERSION="${1:-}"

if [[ -z "$VERSION" ]]; then
  echo "Usage: $0 <version>  (e.g. $0 1.2.0)" >&2
  exit 1
fi

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)"

echo "===> Step 1: Build, package, tag, and GitHub release"
"${SCRIPT_DIR}/release.sh" "$VERSION"

echo
echo "===> Step 2: Homebrew tap update"
"${SCRIPT_DIR}/pkg-homebrew.sh" "$VERSION"

echo
echo "===> Release ${VERSION} complete."
