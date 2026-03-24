#!/usr/bin/env bash
set -euo pipefail

OWNER="bgunnarsson"
REPO="binreq"   # GitHub repo name

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)"
SRC_REPO_DIR="${SCRIPT_DIR}/.."
TAP_DIR="${SCRIPT_DIR}/../../homebrew-binman"

if [ $# -lt 1 ]; then
  echo "Usage: $0 <version-without-v>  (e.g. $0 1.0.0)" >&2
  exit 1
fi

VERSION="$1"
TAG="v${VERSION}"
TARBALL_URL="https://github.com/${OWNER}/${REPO}/archive/refs/tags/${TAG}.tar.gz"

if [ ! -d "$SRC_REPO_DIR" ]; then
  echo "Source repo dir not found: $SRC_REPO_DIR" >&2
  exit 1
fi

if [ ! -d "$TAP_DIR" ]; then
  echo "Homebrew tap repo not found: $TAP_DIR" >&2
  echo "Create it first: gh repo create homebrew-binman --public" >&2
  exit 1
fi

CANDIDATES=(
  "${TAP_DIR}/binman.rb"
  "${TAP_DIR}/Formula/binman.rb"
  "${TAP_DIR}/HomebrewFormula/binman.rb"
)

FORMULA_FILE=""
for f in "${CANDIDATES[@]}"; do
  if [ -f "$f" ]; then
    FORMULA_FILE="$f"
    break
  fi
done

if [ -z "$FORMULA_FILE" ]; then
  echo "Formula file not found in any of:" >&2
  printf '  %s\n' "${CANDIDATES[@]}" >&2
  exit 1
fi

echo "Releasing binman ${VERSION}"
echo "  src repo:     ${SRC_REPO_DIR}"
echo "  tap repo:     ${TAP_DIR}"
echo "  formula file: ${FORMULA_FILE}"
echo "  tag:          ${TAG}"
echo "  tarball:      ${TARBALL_URL}"
echo

cd "$SRC_REPO_DIR"

if ! git diff-index --quiet HEAD --; then
  echo "Working tree is not clean. Commit or stash first." >&2
  exit 1
fi

git fetch --tags >/dev/null 2>&1 || true

if git rev-parse "${TAG}" >/dev/null 2>&1; then
  echo "Tag ${TAG} already exists locally."
else
  echo "Creating tag ${TAG}..."
  git tag -a "${TAG}" -m "binman ${VERSION}"
fi

echo "Pushing tag ${TAG} to origin..."
git push origin "${TAG}"

echo
if command -v gh >/dev/null 2>&1; then
  if gh release view "${TAG}" >/dev/null 2>&1; then
    echo "GitHub release ${TAG} already exists."
  else
    echo "Creating GitHub release ${TAG} via gh..."
    gh release create "${TAG}" \
      --title "binman ${VERSION}" \
      --generate-notes
    echo "GitHub release ${TAG} created."
  fi
else
  echo "gh CLI not found; skipping GitHub Release creation."
fi

echo

TMP_TGZ="$(mktemp)"
echo "Waiting for GitHub to generate tarball..."
sleep 5
echo "Downloading tarball..."
for i in 1 2 3 4 5; do
  curl -L -sSf "$TARBALL_URL" -o "$TMP_TGZ" && break
  echo "  Attempt $i failed, retrying in 5s..."
  sleep 5
done

SHA256="$(shasum -a 256 "$TMP_TGZ" | awk '{print $1}')"
rm -f "$TMP_TGZ"

echo "sha256: ${SHA256}"
echo

perl -pi -e 's|^  url ".*"|  url "'"${TARBALL_URL}"'"|' "$FORMULA_FILE"
perl -pi -e 's|^  sha256 ".*"|  sha256 "'"${SHA256}"'"|' "$FORMULA_FILE"

echo "Updated ${FORMULA_FILE}:"
grep -E 'url "|sha256 "' "$FORMULA_FILE" || true
echo

cd "$TAP_DIR"
echo "Git status in tap repo:"
git status --short
echo

read -r -p "Commit and push these Homebrew changes? [y/N] " ans
if [[ "$ans" =~ ^[Yy]$ ]]; then
  git add "$FORMULA_FILE"
  git commit -m "binman ${VERSION}"
  git push
  echo "Pushed updated formula."
else
  echo "Aborted before commit."
fi

cat <<EOF

Next steps:

  brew uninstall binman        # if installed from this tap
  brew untap ${OWNER}/binman || true
  brew tap ${OWNER}/binman
  brew install ${OWNER}/binman/binman

EOF
