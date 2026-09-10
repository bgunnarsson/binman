#!/usr/bin/env bash
set -euo pipefail

APP="binman"
PKG="./cmd/binman"
OUT_DIR="./dist"

mkdir -p "$OUT_DIR"

export CGO_ENABLED=0

build() {
  local goos="$1"
  local goarch="$2"
  local ext=""
  local bin="${APP}-${goos}-${goarch}"

  if [ "$goos" = "windows" ]; then
    ext=".exe"
  fi

  echo "==> $goos/$goarch"
  GOOS="$goos" GOARCH="$goarch" \
    go build \
      -trimpath \
      -ldflags="-s -w" \
      -o "${OUT_DIR}/${bin}${ext}" \
      "$PKG"
}

build darwin arm64
build darwin amd64
build linux amd64
build linux arm64
build windows amd64
