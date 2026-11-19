#!/usr/bin/env bash
set -euo pipefail

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) echo "darwin-aarch64" ;;
  Darwin-x86_64) echo "darwin-x86-64" ;;
  Linux-x86_64) echo "linux-x86-64" ;;
  Linux-aarch64|Linux-arm64) echo "linux-aarch64" ;;
  MINGW64_NT-*-x86_64|MSYS_NT-*-x86_64|CYGWIN_NT-*-x86_64) echo "win32-x86-64" ;;
  *) echo "unsupported platform: $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac
