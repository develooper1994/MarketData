# Integrating MarketData into AlgoTradePlan

This repository contains the Rust replacement for the Python-centric data layer in `AlgoTradePlan`.

## Migration-compatible contract mapping

| AlgoTradePlan Python | MarketData Rust |
|---|---|
| `DataRequest` | `contracts::DataRequest` |
| `DataRecord` | `contracts::DataRecord` |
| `QualityReport` | `contracts::QualityReport` |
| `StorageReceipt` | `contracts::StorageReceipt` |
| `ProvenanceRecord` | `contracts::ProvenanceRecord` |
| `DataHub.ingest` | `hub::DataHub::ingest` / `ingest_from_raw` |
| `ETL` | `etl::Etl` |

## Practical integration path (single-pass, low risk)

1. Keep existing `AlgoTradePlan` adapter contracts.
2. Add a bridge process/module that calls MarketData for normalize+quality+provenance.
3. Gradually swap Python normalization paths dataset-by-dataset (start with `kline`).

## Suggested immediate changes in AlgoTradePlan

1. Add this repository as an external dependency reference in docs/build notes.
2. Introduce a `MarketDataBridge` in `src/algotradeplan/data/hub.py` that routes selected datasets to Rust.
3. Keep Python fallback enabled behind a feature flag (`use_rust_data_layer`).

See companion snippet files in `integration/algotradeplan/`.
