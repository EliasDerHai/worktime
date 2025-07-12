#!/bin/bash
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)

cd "$ROOT"

cargo build

cp ./target/debug/worktime /opt/homebrew/bin/wt
