#!/usr/bin/env bash
set -euo pipefail

echo "Formatting all Rust code..."
cargo fmt --all

echo "Done."
