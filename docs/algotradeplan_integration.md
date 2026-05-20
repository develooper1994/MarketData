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
| `IngestResult.raw_datasets` | `contracts::IngestResult.raw_datasets` |
| `DataHub.ingest` | `hub::DataHub::ingest` / `ingest_from_raw` |
| `ETL` | `etl::Etl` |

## Verified bridge surface

`src/bin/market_data_bridge.rs` is the supported integration entrypoint for `AlgoTradePlan`.

- `doctor` performs a zero-input setup check and reports the JSON bridge contract.
- `ingest` accepts raw dataset payloads on stdin and returns a JSON `IngestResult` with `raw_datasets`, `records`, `quality_report`, `storage_receipts`, `provenance`, and `source_issues`.
- The output contract is covered by Rust integration tests, including artifact-writing and missing-dataset behavior.

## Practical integration path (single-pass, low risk)

1. Keep existing `AlgoTradePlan` capability/query logic and raw source adapter registry.
2. Replace Python normalize + quality + storage + provenance internals with a `MarketDataBridge` subprocess call.
3. Route `DataHub.ingest()` raw payloads into `market_data_bridge ingest`.
4. Keep the high-level `DataHub`/`ETL` public API unchanged so callers do not notice the migration.
5. Start with `kline` (the implemented Rust dataset) and expand dataset coverage in Rust before deleting more consumer-side compatibility code.

## Suggested immediate changes in AlgoTradePlan

1. Build the Rust bridge binary:

   ```bash
   cargo build --release --bin market_data_bridge
   ```

2. Configure `MARKET_DATA_BIN` to point at the built binary.
3. Introduce the bridge from `integration/algotradeplan/datahub_bridge_example.py` inside `src/algotradeplan/data/hub.py`.
4. Keep capability filtering and raw fetching in Python, but remove the duplicated Python normalize/quality/storage/provenance execution path once `kline` migration checks pass.

## Legacy layer removal target in AlgoTradePlan

After the `MarketDataBridge` is wired in and the existing `AlgoTradePlan` ingestion tests pass against the Rust-backed path, the old duplicated pipeline modules can be removed from `AlgoTradePlan`:

- `src/algotradeplan/data/normalize.py`
- `src/algotradeplan/data/quality.py`
- `src/algotradeplan/data/storage.py`
- `src/algotradeplan/data/provenance.py`

The capability map, query helpers, coverage docs, and raw source adapters should stay until equivalent Rust adapters are implemented in this repository.

See companion snippet files in `integration/algotradeplan/`.
