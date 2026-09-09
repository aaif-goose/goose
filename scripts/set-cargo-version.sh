#!/usr/bin/env bash
set -euo pipefail

version=${1:?Version is required}

sed -i.bak "s/^version = \".*\"/version = \"${version}\"/" Cargo.toml
rm -f Cargo.toml.bak
