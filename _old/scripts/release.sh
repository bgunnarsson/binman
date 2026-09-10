#!/usr/bin/env bash
set -euo pipefail

# Usage: ./scripts/release.sh 1.0.0
VERSION="${1:-}"

if [[ -z "$VERSION" ]]; then
  echo "Usage: $0 <version>" >&2
  exit 1
fi

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

DIST_DIR="dist"

BIN_DARWIN_ARM64="binman-darwin-arm64"
BIN_DARWIN_AMD64="binman-darwin-amd64"
BIN_LINUX_AMD64="binman-linux-amd64"
BIN_WINDOWS_AMD64="binman-windows-amd64.exe"

echo "==> Building binaries"
if [[ -x "scripts/build.sh" ]]; then
  scripts/build.sh
else
  echo "scripts/build.sh not found or not executable; build manually first." >&2
  exit 1
fi

echo "==> Packaging archives for ${VERSION}"
cd "$DIST_DIR"

for f in "$BIN_DARWIN_ARM64" "$BIN_DARWIN_AMD64" "$BIN_LINUX_AMD64" "$BIN_WINDOWS_AMD64"; do
  if [[ ! -f "$f" ]]; then
    echo "Missing expected binary: $f" >&2
    exit 1
  fi
done

rm -f \
  "binman-${VERSION}-darwin-arm64.tar.gz" \
  "binman-${VERSION}-darwin-amd64.tar.gz" \
  "binman-${VERSION}-linux-amd64.tar.gz" \
  "binman-${VERSION}-windows-amd64.zip"

tar -czf "binman-${VERSION}-darwin-arm64.tar.gz" "$BIN_DARWIN_ARM64"
tar -czf "binman-${VERSION}-darwin-amd64.tar.gz" "$BIN_DARWIN_AMD64"
tar -czf "binman-${VERSION}-linux-amd64.tar.gz"  "$BIN_LINUX_AMD64"
zip -q   "binman-${VERSION}-windows-amd64.zip"   "$BIN_WINDOWS_AMD64"

cd "$ROOT"

echo "==> Git commit + tag ${VERSION}"
git add .
git commit -m "Release ${VERSION}"
git tag "${VERSION}"

echo "==> Pushing branch and tag"
git push
git push origin "${VERSION}"

echo "==> Creating GitHub release ${VERSION}"
gh release create "${VERSION}" \
  "dist/binman-${VERSION}-darwin-arm64.tar.gz" \
  "dist/binman-${VERSION}-darwin-amd64.tar.gz" \
  "dist/binman-${VERSION}-linux-amd64.tar.gz" \
  "dist/binman-${VERSION}-windows-amd64.zip" \
  --title "${VERSION}" \
  --notes "Release ${VERSION}"

echo "==> Done."
