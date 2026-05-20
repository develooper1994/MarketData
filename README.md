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

Cross-repository write access is not available in this workspace, so the integration-ready companion instructions are provided here:

- `docs/algotradeplan_integration.md`
- `integration/algotradeplan/pyproject_snippet.toml`
- `integration/algotradeplan/datahub_bridge_example.py`

These files show the minimal changes needed in `AlgoTradePlan` to start consuming this Rust data layer.
