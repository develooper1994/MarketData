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

`README` is the primary entrypoint: use it for setup, first CLI runs, and links to
the canonical docs below.

### Bridge CLI (quick use)

```bash
# detailed command menu + common flows
cargo run --quiet --bin market_data_bridge -- help

# contract/health check
cargo run --quiet --bin market_data_bridge -- doctor
cargo run --quiet --bin market_data_bridge -- assert-contract --expected 1

# source discovery
cargo run --quiet --bin market_data_bridge -- sources
cargo run --quiet --bin market_data_bridge -- query-sources-for \
  --dataset kline --asset-class crypto_spot

# recommendation + explain
cargo run --quiet --bin market_data_bridge -- query-best-sources \
  --dataset kline --asset-class crypto_spot --limit 5
cargo run --quiet --bin market_data_bridge -- query-source-summary --source binance_futures
cargo run --quiet --bin market_data_bridge -- query-dataset-summary --dataset kline
cargo run --quiet --bin market_data_bridge -- recommend-sources --use-case crypto_backtest --limit 5
cargo run --quiet --bin market_data_bridge -- supported-use-cases

# ingest (normalize + quality + storage + provenance)
printf '{"kline":[[1716200000000,"10","11","9","10.5","42"]]}' | \
  cargo run --quiet --bin market_data_bridge -- ingest \
    --source offline \
    --symbol BTCUSDT \
    --datasets kline \
    --asset-type crypto_spot
```

For a concise CLI command reference and common workflows:
[`docs/cli_usage.md`](docs/cli_usage.md)

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

## Canonical documentation set

Keep docs intentionally small and non-duplicative:

| Doc | Purpose |
|---|---|
| [`docs/standalone_data_layer_migration_plan.md`](docs/standalone_data_layer_migration_plan.md) | Authoritative architecture and migration ownership model |
| [`docs/cli_usage.md`](docs/cli_usage.md) | Bridge CLI menu, command cheatsheet, and common flows |
| [`docs/algotradeplan_integration.md`](docs/algotradeplan_integration.md) | AlgoTradePlan bridge wiring and cutover integration notes |
| [`docs/new_project_onboarding.md`](docs/new_project_onboarding.md) | Reusable onboarding for any new consumer project |
| [`docs/data_layer_status_gap_analysis.md`](docs/data_layer_status_gap_analysis.md) | Current readiness, risk notes, and actionable gap checklist |

## Data source support (24 sources)

`binance_futures`, `bybit_linear`, `kraken_spot`, `coinbase_spot`,
`yahoo_unofficial`, `alpha_vantage`, `twelve_data`, `polygon_io`,
`finnhub`, `quandl`, `iex_cloud`, `frankfurter_fx`, `coingecko`,
`stooq`, `fred`, `gdelt`, `financial_modeling_prep`, `sec_edgar`,
`world_bank`, `ecb`, `defillama`, `hacker_news`, `tefas_public`,
`offline_fallback`
