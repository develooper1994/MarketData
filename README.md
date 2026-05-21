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
| `prelude` | Re-exports all commonly used types for easy `use market_data::prelude::*` import |

## Quick start

```bash
cargo test
```

Live online fetch doğrulamasini precommit asamasinda calistirmak icin:

```bash
./scripts/precommit_live_check.sh
```

Bu komut `MARKET_DATA_LIVE_TESTS=1` ile canli adaptor matrisi testini kosar. Varsayilan `cargo test` offline-safe kalir.

`README` is the primary entrypoint: use it for setup, first CLI runs, and links to
the canonical docs below.

### Bridge CLI (quick use)

```bash
# detailed command menu + common flows
cargo run --quiet --bin market_data_bridge -- help
# short aliases are supported too (for speed): ls, qsf, qbs, qss, qds, qdm, rs, suc, ing

# contract/health check
cargo run --quiet --bin market_data_bridge -- doctor
cargo run --quiet --bin market_data_bridge -- assert-contract --expected 1
# commandless stdin-json request mode (thin client parity)
printf '{"command":"doctor"}' | cargo run --quiet --bin market_data_bridge --

# source discovery
cargo run --quiet --bin market_data_bridge -- sources
cargo run --quiet --bin market_data_bridge -- query-sources-for \
  --dataset kline --asset-class crypto_spot

# recommendation + explain
cargo run --quiet --bin market_data_bridge -- query-best-sources \
  --dataset kline --asset-class crypto_spot --limit 5
cargo run --quiet --bin market_data_bridge -- query-source-summary --source binance_futures
cargo run --quiet --bin market_data_bridge -- query-dataset-summary --dataset kline
cargo run --quiet --bin market_data_bridge -- query-dataset-matrix
cargo run --quiet --bin market_data_bridge -- recommend-sources --use-case crypto_backtest --limit 5
cargo run --quiet --bin market_data_bridge -- supported-use-cases

# single-command online fetch (no printf/stdin JSON)
cargo run --quiet --bin market_data_bridge -- live-fetch \
   --source binance_futures --symbol BTCUSDT --dataset tick --limit 5
cargo run --quiet --bin market_data_bridge -- live-fetch \
   --source binance_futures --symbol BTCUSDT --datasets tick,funding --asset-type crypto_perp --limit 5

# ingest (normalize + quality + storage + provenance)
# Example: ingest historical candle data (timestamp in milliseconds)
printf '{"kline":[[1716200000000,"10","11","9","10.5","42"]]}' | \
  cargo run --quiet --bin market_data_bridge -- ingest \
    --source offline \
    --symbol BTCUSDT \
    --datasets kline \
    --asset-type crypto_spot
```

### CLI command cheatsheet

| Command | Purpose |
|---|---|
| `doctor` | Health/version/contract metadata |
| `assert-contract --expected <version>` | Fail-fast contract compatibility gate |
| `sources` | Source name list |
| `capabilities` | Full source capability metadata |
| `query-sources-for --dataset <name> [--asset-class <name>] [--require-live]` | Filter sources by requirements |
| `query-best-sources --dataset <name> [--asset-class <name>] [--limit N]` | Ranked source recommendations |
| `query-source-summary --source <name>` | Human-readable source summary |
| `query-dataset-summary --dataset <name>` | Dataset-level coverage summary |
| `query-dataset-matrix` | Machine-readable dataset-to-source coverage matrix |
| `supported-use-cases` | Built-in recommendation flows |
| `recommend-sources --use-case <name> [--limit N]` | Use-case recommendation list |
| `live-fetch --source <name> --symbol <id> --dataset <name>` | Single-command real online fetch |
| `ingest --source <name> --symbol <id> --datasets <csv>` | Normalize + quality + storage + provenance |

Short aliases: `status`, `assert`, `caps`, `ls`, `qsf`, `qbs`, `qss`, `qds`, `qdm`, `rs`, `suc`, `lf`, `ing`

### Common CLI flows

1. Verify runtime + contract:
   - `market_data_bridge doctor`
   - `market_data_bridge assert-contract --expected 1`
2. Discover source coverage:
   - `market_data_bridge sources`
   - `market_data_bridge capabilities`
   - `market_data_bridge query-sources-for --dataset kline --asset-class crypto_spot`
3. Select recommended sources:
   - `market_data_bridge query-best-sources --dataset kline --asset-class crypto_spot --limit 5`
   - `market_data_bridge query-source-summary --source binance_futures`
   - `market_data_bridge query-dataset-summary --dataset kline`
   - `market_data_bridge query-dataset-matrix`
   - `market_data_bridge supported-use-cases`
   - `market_data_bridge recommend-sources --use-case crypto_backtest --limit 5`
4. Fetch online data with one command:
   - `market_data_bridge live-fetch --source binance_futures --symbol BTCUSDT --dataset tick --limit 5`
   - `market_data_bridge live-fetch --source binance_futures --symbol BTCUSDT --datasets tick,funding --asset-type crypto_perp --limit 5`
5. Run full ingest pipeline:
   - `printf '{"kline":[[1716200000000,100,110,90,105,1000]]}' | market_data_bridge ingest --source offline --symbol BTCUSDT --datasets kline --asset-type crypto_spot`
   - `market_data_bridge ingest --source offline --symbol BTCUSDT --datasets kline --asset-type crypto_spot` (with empty stdin uses deterministic offline adapter payload)

### Library usage

```rust
// Short-form import using the prelude (recommended):
use market_data::prelude::*;

// Or import individual types:
use market_data::{DataHub, InMemoryStorage, ManifestProvenanceTracker, SourceAdapterRegistry};

let mut hub = DataHub::with_components(
    Box::new(InMemoryStorage::default()),
    ManifestProvenanceTracker::new(None::<&str>),
    SourceAdapterRegistry::default(),
);

// Example: library ingest of historical candle data (timestamp in milliseconds)
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
| [`docs/STATUS.md`](docs/STATUS.md) | Current readiness checklist and actionable TODOs |
| [`docs/standalone_data_layer_migration_plan.md`](docs/standalone_data_layer_migration_plan.md) | Architecture, ownership model, integration patterns, cutover steps |
| [`docs/algotradeplan_integration.md`](docs/algotradeplan_integration.md) | AlgoTradePlan bridge wiring and contract mapping |
| [`docs/new_project_onboarding.md`](docs/new_project_onboarding.md) | Reusable onboarding for any new consumer project |
| [`integration/algotradeplan/migration_cutover.md`](integration/algotradeplan/migration_cutover.md) | Step-by-step destructive migration guide for AlgoTradePlan |

## Data source support (24 sources)

`binance_futures`, `binance_spot`, `bybit_linear`, `kraken_spot`, `coinbase_spot`,
`yahoo_unofficial`, `alpha_vantage`, `twelve_data`, `polygon_io`,
`finnhub`, `quandl`, `iex_cloud`, `frankfurter_fx`, `coingecko`,
`stooq`, `fred`, `gdelt`, `financial_modeling_prep`, `sec_edgar`,
`world_bank`, `ecb`, `defillama`, `hacker_news`, `tefas_public`,
`offline_fallback`
