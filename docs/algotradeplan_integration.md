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

Canonical CLI usage now lives in the main quick-start:
[`README.md`](../README.md#bridge-cli-quick-use)

Build + binary setup for AlgoTradePlan integration:

```bash
cargo build --release --bin market_data_bridge
export MARKET_DATA_BIN="$PWD/target/release/market_data_bridge"
```

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
