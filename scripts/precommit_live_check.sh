#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

export MARKET_DATA_LIVE_TESTS=1

echo "[precommit-live] building market_data_bridge"
cargo build --release --bin market_data_bridge

echo "[precommit-live] running live adapter matrix test"
cargo test live_adapters_are_opt_in_and_do_not_crash -- --nocapture

echo "[precommit-live] running bridge contract checks"
./target/release/market_data_bridge doctor >/dev/null
./target/release/market_data_bridge assert-contract --expected 1 >/dev/null

echo "[precommit-live] OK"
