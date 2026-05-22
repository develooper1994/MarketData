#!/usr/bin/env bash
set -eo pipefail

ROOT="/home/developer/Projects/github/Project/MarketData"
cd "$ROOT" || { echo "Directory not found: $ROOT"; exit 1; }

mkdir -p ingest_outputs

labels=(yahoo btcturk kap fintables tradingview paratic dovizcom tefas)
cmds=(
  "cargo run --quiet --bin market_data_bridge -- ingest --source yahoo --symbol AAPL --datasets tick"
  "cargo run --quiet --bin market_data_bridge -- ingest --source btcturk --symbol BTCUSDT --datasets tick"
  "cargo run --quiet --bin market_data_bridge -- ingest --source kap --symbol GARAN --datasets news"
  "ENABLE_SCRAPING_PROVIDERS=true cargo run --quiet --bin market_data_bridge -- ingest --source fintables --symbol GARAN --datasets fundamentals"
  "cargo run --quiet --bin market_data_bridge -- ingest --source tradingview --symbol BTCUSDT --datasets tick"
  "cargo run --quiet --bin market_data_bridge -- ingest --source paratic --symbol BTCUSDT --datasets tick"
  "cargo run --quiet --bin market_data_bridge -- ingest --source dovizcom --symbol USDTRY --datasets tick"
  "cargo run --quiet --bin market_data_bridge -- ingest --source tefas --symbol 0001 --datasets fundamentals"
)

for i in "${!labels[@]}"; do
  label="${labels[$i]}"
  cmd="${cmds[$i]}"
  out="ingest_outputs/${label}.log"

  echo "=== [${label}] START ==="
  # run and capture both stdout/stderr
  bash -c "$cmd" > "$out" 2>&1 || true
  rc=$?

  if [ $rc -eq 0 ]; then
    echo "=== [${label}] SUCCESS ==="
    echo "---- OUTPUT (${out}) ----"
    sed -n '1,200p' "$out" || true
  else
    echo "=== [${label}] FAILED exit=$rc ==="
    echo "---- OUTPUT (${out}) ----"
    sed -n '1,200p' "$out" || true
  fi
  echo
done

echo "Per-provider outputs saved in: ingest_outputs/"
