#!/bin/bash
set -euo pipefail

cd $(git rev-parse --show-toplevel) 

touch comptime.db
touch worktime.db

cargo build
