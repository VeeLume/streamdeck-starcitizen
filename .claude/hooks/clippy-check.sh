#!/usr/bin/env bash
set -euo pipefail
cargo clippy --all-targets --all-features 2>&1 || true
