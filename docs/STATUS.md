# MarketData — Status and Roadmap Checklist

Quick-reference status for the single-data-layer migration.

> Full architecture, ownership model, and integration guides:
> see [`README.md`](../README.md) and the docs below.

---

## What is done ✅

- [x] Canonical normalization for 9 dataset types (`kline`, `tick`, `trade`, `orderbook`, `funding`, `macro`, `news`, `fundamentals`, `corporate_actions`)
- [x] Quality checks: required fields, monotonic timestamps, non-negative OHLCV
- [x] In-memory and local JSONL artifact storage
- [x] Provenance manifest tracker
- [x] Rust ingestion pipeline: normalize → quality → storage → provenance (`DataHub`)
- [x] Fluent ETL façade (`Etl`)
- [x] 24-source capability registry (`src/capabilities.rs`)
- [x] Source query/recommendation helpers (`src/query.rs`)
- [x] Machine-readable dataset/source coverage matrix (`query-dataset-matrix`)
- [x] Canonical data contracts (`src/contracts.rs`)
- [x] Offline reference adapter registered by default for safe local fetch/discovery smoke paths (`SourceAdapterRegistry::default()`)
- [x] `market_data_bridge` CLI with `help`, `doctor`, `assert-contract`, `sources`, `capabilities`, `query-sources-for`, `query-best-sources`, `query-source-summary`, `query-dataset-summary`, `supported-use-cases`, `recommend-sources`, `ingest`
- [x] Short command aliases (`ls`, `qsf`, `qbs`, `qss`, `qds`, `rs`, `suc`, `ing`, `status`, `assert`, `caps`)
- [x] Bridge contract versioning (`BRIDGE_CONTRACT_VERSION = "1"`)
- [x] AlgoTradePlan compatibility shim (`integration/algotradeplan/hub_bridge.py`)
- [x] Destructive migration guide (`integration/algotradeplan/migration_cutover.md`)
- [x] New-project onboarding guide (`docs/new_project_onboarding.md`)
- [x] `prelude` module for easy Rust API imports (`use market_data::prelude::*`)
- [x] Full CLI test coverage (31 tests passing)
- [x] `cargo fmt` / `cargo clippy` / `cargo test` all pass

---

## What is pending / actionable TODOs ⬜

### Provider / Adapter runtime

- [ ] **Replace offline reference defaults with production-grade provider adapters** in `SourceAdapterRegistry::default()` so live fetch works without a caller-supplied payload (e.g. hardened Binance, Yahoo, or TEFAS HTTP adapters)
- [ ] **Add live-fetch path** to `DataHub` / bridge `ingest` — today, ingestion requires caller to supply raw JSON; a real adapter fetch path needs to be wired
- [ ] **Retry / backoff / rate-limiting** for live HTTP adapters
- [ ] **Credentials / config model** — how live adapters discover API keys (env vars, config file, or secret store)

### Storage / Persistence

- [ ] **Production storage backends** beyond in-memory and local JSONL (e.g. SQLite, S3, or a time-series store)
- [ ] **Retention and replayability policy** — how long are artifacts kept, how are they replayed

### Observability / Reliability

- [ ] **Structured logging** (tracing / log crate) throughout the pipeline
- [ ] **Structured error types** instead of `Box<dyn Error>` in bridge and hub
- [ ] **Health check** endpoint for gRPC / HTTP mode (Phase 3)

### Testing and fixtures

- [ ] **Full fixture coverage** for all 24 sources with representative payloads
- [ ] **Integration tests with AlgoTradePlan** — run `hub_bridge.py` smoke tests in MarketData CI
- [ ] **Parity regression tests** — round-trip parity for all consumer-critical dataset/use-case combinations

### Client SDK / Service

- [ ] **Versioned Python client package** (`market_data_client`) on PyPI or as a git-installable package
- [ ] **gRPC microservice (Phase 3)** — wrap bridge commands behind a `tonic` server so consumers can call over the network
- [ ] **Release and version policy** — tag `v0.x.y` releases, changelog, semantic versioning policy

### Consumer integration

- [ ] **Cross-project validation** — at least one non-AlgoTradePlan consumer integrated and passing in CI
- [ ] **AlgoTradePlan cutover verification** — confirm AlgoTradePlan data layer removed and all tests pass through the shim

---

## Validation commands

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

All must pass before any consumer cutover.
