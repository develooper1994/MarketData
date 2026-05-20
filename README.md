# MarketData

**Standalone, reusable data-layer platform.**

`MarketData` is the single authoritative data layer for any project that needs
market data ingestion, normalization, quality validation, storage, provenance
tracking, and source capability discovery.  It is not tied to any single
consumer: `AlgoTradePlan`, future trading systems, analytics pipelines, and
research tooling all consume it through its public surfaces (Rust crate, bridge
CLI, or the planned gRPC service).

All data-layer responsibilities (normalize, quality, storage, provenance,
capability/query logic, adapter-facing integration surface, and provider
registry) live exclusively here.  Consumer projects keep at most a thin
compatibility shim that delegates every data decision to `MarketData`.

## Modules

| Module | Responsibility |
|---|---|
| `capabilities` | 24-source registry with full metadata (replaces `AlgoTradePlan/data/capabilities.py`) |
| `query` | Source-for-dataset filtering, best-source ranking, dataset-status lookup |
| `contracts` | Request / record / report / provenance / receipt structs |
| `normalize` | 9 dataset normalizers: `kline`, `tick`, `trade`, `orderbook`, `funding`, `macro`, `news`, `fundamentals`, `corporate_actions` |
| `quality` | Canonical validation: required fields, monotonic timestamps, non-negative OHLCV |
| `storage` | In-memory and local JSONL artifact writers |
| `provenance` | Manifest tracker for ingestion lineage |
| `hub` | Orchestrates normalize → quality → storage → provenance |
| `etl` | Fluent façade over `DataHub` |

## Quick start

```bash
cargo test
```

### Bridge CLI commands

```bash
# verify setup
cargo run --quiet --bin market_data_bridge -- doctor

# list all 24 sources
cargo run --quiet --bin market_data_bridge -- sources

# filter sources by dataset + asset class
cargo run --quiet --bin market_data_bridge -- query-sources-for \
  --dataset kline --asset-class crypto_spot

# full source capability metadata
cargo run --quiet --bin market_data_bridge -- capabilities

# ingest (normalize + quality + storage + provenance)
printf '{"kline":[[1716200000000,"10","11","9","10.5","42"]]}' | \
  cargo run --quiet --bin market_data_bridge -- ingest \
    --source offline \
    --symbol BTCUSDT \
    --datasets kline \
    --asset-type crypto_spot
```

### Library usage

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

## Consumer projects

| Consumer | Integration pattern | Key file |
|---|---|---|
| `AlgoTradePlan` | Subprocess bridge | `integration/algotradeplan/hub_bridge.py` |
| Any Python/polyglot project | Subprocess bridge | `docs/new_project_onboarding.md` |
| Any Rust project | Crate dependency | `docs/new_project_onboarding.md` |
| Future gRPC client | Network call (Phase 3) | — |

## Architecture and migration plan

The authoritative document for the full migration and target architecture is:

> **[`docs/standalone_data_layer_migration_plan.md`](docs/standalone_data_layer_migration_plan.md)**

It covers: target ownership model, adapter/provider architecture, client
surfaces, migration sequence, acceptance criteria, documentation requirements,
and validation requirements.

## AlgoTradePlan migration

The destructive data-layer migration has been completed.  Key integration files:

| File | Purpose |
|---|---|
| `integration/algotradeplan/hub_bridge.py` | Drop-in replacement for `AlgoTradePlan/src/algotradeplan/data/hub.py` |
| `integration/algotradeplan/migration_cutover.md` | Step-by-step migration guide |
| `integration/algotradeplan/datahub_bridge_example.py` | Standalone usage example |
| `docs/algotradeplan_integration.md` | Architecture overview and CLI reference |
| `docs/standalone_data_layer_migration_plan.md` | Authoritative standalone migration + reusable architecture plan |
| `docs/new_project_onboarding.md` | How to use MarketData from any project |

### Files removed from AlgoTradePlan

| Deleted | Rust replacement |
|---|---|
| `data/normalize.py` | `src/normalize.rs` |
| `data/quality.py` | `src/quality.rs` |
| `data/storage.py` | `src/storage.rs` |
| `data/provenance.py` | `src/provenance.rs` |
| `data/capabilities.py` | `src/capabilities.rs` |
| `data/query.py` | `src/query.rs` + `integration/algotradeplan/hub_bridge.py` |
| `data/coverage.py` | `integration/algotradeplan/hub_bridge.py` |
| `data/adapters/` | `integration/algotradeplan/hub_bridge.py` adapter-facing surface |

## Rust migration roadmap

| Phase | Status | Description |
|---|---|---|
| 1 | **Done** | Bridge CLI: `ingest`, `capabilities`, `sources`, `query-sources-for` |
| 2 | **Done** | Remove AlgoTradePlan query/coverage ownership; keep thin bridge shim |
| 3 | Planned | Expose bridge as gRPC microservice (`tonic`) |
| 4 | Planned | Rust hot path: indicators, backtest core |

## Data source support (24 sources)

`binance_futures`, `bybit_linear`, `kraken_spot`, `coinbase_spot`,
`yahoo_unofficial`, `alpha_vantage`, `twelve_data`, `polygon_io`,
`finnhub`, `quandl`, `iex_cloud`, `frankfurter_fx`, `coingecko`,
`stooq`, `fred`, `gdelt`, `financial_modeling_prep`, `sec_edgar`,
`world_bank`, `ecb`, `defillama`, `hacker_news`, `tefas_public`,
`offline_fallback`
