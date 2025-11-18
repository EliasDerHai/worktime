#!/bin/bash
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)

cd "$ROOT"

cargo build --release

cp ./target/release/worktime /opt/homebrew/bin/wt
