# MarketData

Rust-native market data ingestion and normalization layer extracted from the `AlgoTradePlan` data architecture.

## Why this project exists

`AlgoTradePlan` currently implements a Python `DataHub` pipeline (`fetch -> normalize -> quality -> storage -> provenance`).
This repository now hosts the migration-friendly Rust implementation of that same pipeline so performance-sensitive ingestion can evolve independently.

## Implemented modules

- `contracts`: request/record/report/provenance structs
- `normalize`: dataset normalization (`kline` implemented, extensible)
- `quality`: canonical validation checks (required fields, monotonic timestamps, non-negative OHLCV)
- `storage`: in-memory and local artifact writers
- `provenance`: manifest tracker for ingestion lineage
- `hub`: orchestrates normalize -> quality -> storage -> provenance
- `etl`: fluent facade over `DataHub`

## Quick start

```bash
cargo test
```

Bridge smoke check for `AlgoTradePlan`-style subprocess integration:

```bash
cargo run --quiet --bin market_data_bridge -- doctor
printf '{"kline":[[1716200000000,"10","11","9","10.5","42"]]}' | \
  cargo run --quiet --bin market_data_bridge -- ingest \
    --source offline \
    --symbol BTCUSDT \
    --datasets kline \
    --asset-type crypto_spot
```

Example (library usage):

```rust
use market_data::{DataHub, InMemoryStorage, ManifestProvenanceTracker, SourceAdapterRegistry};

let mut hub = DataHub::with_components(
    Box::new(InMemoryStorage::default()),
    ManifestProvenanceTracker::new(None::<&str>),
    SourceAdapterRegistry::default(),
);

let result = hub.ingest_from_raw(
    "offline",
    "BTCUSDT",
    vec!["kline".to_string()],
    std::collections::HashMap::from([(
        "kline".to_string(),
        serde_json::json!([[1716200000000_i64, "10", "11", "9", "10.5", "42"]]),
    )]),
    true,
)?;
```

## Data source roadmap (for future adapters)

This project is intentionally adapter-agnostic. Planned adapters can include:

- BIST vendor APIs (including VERDA-capable feeds where contractually available)
- KAP disclosures
- TEFAS fund data
- Global fallback sources (Yahoo/Google/MSN style web sources, with strict reliability flags)

## AlgoTradePlan integration

Cross-repository write access is not available in this workspace, so the integration-ready companion instructions and verified bridge entrypoint are provided here:

- `docs/algotradeplan_integration.md`
- `integration/algotradeplan/pyproject_snippet.toml`
- `integration/algotradeplan/datahub_bridge_example.py`
- `src/bin/market_data_bridge.rs`

These files show the minimal changes needed in `AlgoTradePlan` to replace its duplicated normalize/quality/storage/provenance implementation with `MarketData` while keeping existing source capability and raw-fetch logic until Rust adapters are added.
