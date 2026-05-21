# Integrating MarketData into AlgoTradePlan

`develooper1994/MarketData` is the **authoritative data-layer project**.
AlgoTradePlan has been migrated in a single destructive pass to depend on
this Rust crate rather than maintaining its own data layer.

For the full standalone ownership model, destructive cutover sequence, and
multi-project reuse target architecture, see
[`docs/standalone_data_layer_migration_plan.md`](./standalone_data_layer_migration_plan.md).

---

## Architecture overview

```
AlgoTradePlan (Python)
│
├── data/hub.py              ← thin compatibility shim  ────────────────┐
│                                                                        │
│  subprocess (stdin JSON → stdout JSON)                                 │
▼                                                                        │
market_data_bridge (Rust)  ◄───────────────────────────────────────────┘
    ├── capabilities         → 24-source registry, dataset status, rankings
    ├── sources              → source name list
    ├── query-sources-for    → filtered source lookup
    └── ingest               → normalize + quality + storage + provenance
```

---

## Contract mapping

| AlgoTradePlan Python | MarketData Rust |
|---|---|
| `DataRequest` | `contracts::DataRequest` |
| `DataRecord` | `contracts::DataRecord` |
| `QualityReport` | `contracts::QualityReport` |
| `StorageReceipt` | `contracts::StorageReceipt` |
| `ProvenanceRecord` | `contracts::ProvenanceRecord` |
| `IngestResult` | `contracts::IngestResult` |
| `DataHub.ingest` | bridge `ingest` command |
| `DataHub.sources()` | bridge `sources` command |
| `DataHub.capability(src)` | bridge `capabilities` command |
| `DataHub.sources_for(...)` | bridge `query-sources-for` command |
| `SourceCapability` metadata | `capabilities::SourceCapability` (24 sources) |
| `normalize_dataset(...)` | `normalize::normalize_dataset` |
| `CanonicalDataQualityPlugin` | `quality::CanonicalDataQuality` |
| `LocalArtifactStorage` | `storage::LocalArtifactStorage` |
| `ManifestProvenanceTracker` | `provenance::ManifestProvenanceTracker` |

---

## Bridge CLI reference

Build the binary first:

```bash
cargo build --release --bin market_data_bridge
export MARKET_DATA_BIN="$PWD/target/release/market_data_bridge"
```

### `doctor` – verify setup

```bash
market_data_bridge doctor
```

Returns JSON with `status`, `version`, `supported_datasets` (9 types), and
`bridge_contract` flags.

### `assert-contract` – fail fast on incompatible bridge versions

```bash
market_data_bridge assert-contract --expected 1
```

Returns `{"status":"ok","compatible":true,...}` when versions match. Exits
non-zero when the expected and actual contract versions differ.

### `capabilities` – full source registry

```bash
market_data_bridge capabilities
```

Returns a JSON array of 24 `SourceCapability` objects.

### `sources` – source name list

```bash
market_data_bridge sources
```

Returns a JSON array of source names.

### `query-sources-for` – filtered source lookup

```bash
market_data_bridge query-sources-for --dataset kline
market_data_bridge query-sources-for --dataset kline --asset-class crypto_spot
market_data_bridge query-sources-for --dataset tick --require-live
```

### `query-best-sources` – ranked source recommendations for a dataset

```bash
market_data_bridge query-best-sources --dataset kline --asset-class crypto_spot --limit 5
market_data_bridge query-best-sources --dataset fundamentals --include-metadata-only
```

### `query-source-summary` – explain one source

```bash
market_data_bridge query-source-summary --source binance_futures
```

### `query-dataset-summary` – explain one dataset

```bash
market_data_bridge query-dataset-summary --dataset kline
```

### `recommend-sources` / `supported-use-cases` – use-case level recommendations

```bash
market_data_bridge supported-use-cases
market_data_bridge recommend-sources --use-case crypto_backtest --limit 5
```

### `ingest` – normalize / quality / storage / provenance

```bash
echo '{"kline": [[1716000000000,100,110,90,105,1000]]}' | \
  market_data_bridge ingest \
    --source binance_futures \
    --symbol BTCUSDT \
    --datasets kline \
    --store \
    --record-root ./artifacts/records \
    --manifest-root ./artifacts/manifests
```

Returns a JSON `IngestResult` with `records`, `quality_report`,
`storage_receipts`, `provenance`, and `source_issues`.

Supported dataset types for `ingest`: `kline`, `tick`, `trade`, `orderbook`,
`funding`, `macro`, `news`, `fundamentals`, `corporate_actions`.

---

## Cutover instructions

See [`integration/algotradeplan/migration_cutover.md`](../integration/algotradeplan/migration_cutover.md)
for the complete step-by-step destructive migration guide.

Key files:

| File | Purpose |
|---|---|
| `integration/algotradeplan/hub_bridge.py` | Drop-in `hub.py` replacement for AlgoTradePlan |
| `integration/algotradeplan/migration_cutover.md` | Step-by-step cutover guide |
| `integration/algotradeplan/datahub_bridge_example.py` | Standalone usage example |

---

## Files removed from AlgoTradePlan

| File | Reason |
|---|---|
| `data/normalize.py` | Replaced by `src/normalize.rs` |
| `data/quality.py` | Replaced by `src/quality.rs` |
| `data/storage.py` | Replaced by `src/storage.rs` |
| `data/provenance.py` | Replaced by `src/provenance.rs` |
| `data/capabilities.py` | Replaced by `src/capabilities.rs` |
| `data/query.py` | Replaced by MarketData-owned capability/query surfaces |
| `data/coverage.py` | Replaced by bridge-backed capability coverage table |
| `data/adapters/` | Replaced by MarketData adapter-facing integration surfaces |

---

## Rust migration roadmap

| Phase | Status | Description |
|---|---|---|
| 1 | **Done** | Bridge CLI: ingest, capabilities, sources, query-sources-for |
| 2 | **Done** | Remove AlgoTradePlan query/coverage ownership; keep only bridge shim |
| 2.1 | **Done** | Query/recommend/explain commands added to bridge (`query-best-sources`, `query-source-summary`, `query-dataset-summary`, `recommend-sources`) |
| 3 | Planned | Expose bridge as gRPC microservice (`tonic`) |
| 4 | Planned | Rust hot-path: indicators, backtest core |
