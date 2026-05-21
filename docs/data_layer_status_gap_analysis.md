# MarketData Data-Layer Status & Gap Analysis

_Last updated: 2026-05-21_

## Executive status

**Short answer:** `MarketData` is strong on canonical processing and integration contracts, but it is **not yet a complete drop-in replacement for every data-layer responsibility** across `AlgoTradePlan` and future projects.

It can already be used as the core data platform for normalize/quality/storage/provenance and capability discovery/query, but a full cutover still needs adapter/runtime and platform-hardening work.

---

## 1) What is fully covered today

The following responsibilities are implemented in this repository and validated by tests:

- **Canonical normalization** (`src/normalize.rs`) for 9 dataset types.
- **Quality checks** (`src/quality.rs`) including monotonic timestamp and OHLCV checks.
- **Storage layer** (`src/storage.rs`) with in-memory + local JSONL artifact support.
- **Provenance tracking** (`src/provenance.rs`) with manifest capture.
- **Rust ingestion orchestration** (`src/hub.rs`, `src/etl.rs`): normalize → quality → storage → provenance.
- **Canonical contracts** (`src/contracts.rs`) and bridge contract pinning (`BRIDGE_CONTRACT_VERSION = "1"` in `src/bin/market_data_bridge.rs`).
- **Capability registry + query helpers** (`src/capabilities.rs`, `src/query.rs`) with 24-source metadata model.
- **Bridge CLI for polyglot integration** (`doctor`, `assert-contract`, `sources`, `capabilities`, `query-sources-for`, `ingest`).
- **AlgoTradePlan compatibility shim** (`integration/algotradeplan/hub_bridge.py`) for subprocess-based integration.

---

## 2) What is partial or missing

### A) Provider/adapter runtime coverage is incomplete

- `SourceAdapterRegistry` is generic and extensible, but **no production adapter set is registered by default** in Rust (`SourceAdapterRegistry::default()` is empty unless callers register adapters).
- Bridge `ingest` currently consumes caller-supplied raw JSON datasets; it is not yet a full “fetch from provider + ingest” runtime by itself.
- Capability matrix includes many sources with statuses like `partial`, `api_key`, `api_key_or_plan`, `fallback`, and metadata-only datasets, which indicates mixed readiness.

### B) Query/recommendation centralization is improved but still evolving

- Bridge now exposes first-class query/recommend/explain commands (`query-best-sources`, `query-source-summary`, `query-dataset-summary`, `recommend-sources`, `supported-use-cases`).
- Compatibility shim can delegate these flows to Rust bridge commands.
- Remaining work is operational hardening and richer policy/configuration for project-specific recommendation profiles over time.

### C) Multi-project platform completeness is not finished

- No gRPC service yet (explicitly Phase 3 planned).
- Storage backends are currently in-memory + local artifacts; no first-class DB backends in-repo yet.
- Cross-project fixture/contract matrix exists for bridge behaviors, but not yet a broad “multi-consumer certification” suite.

---

## 3) Migration state and cutover caveats

- The repository has strong migration docs and a working compatibility bridge.
- However, **immediate removal of the entire external data-layer surface from consumers is only safe when their remaining raw-fetch/provider logic is also moved behind MarketData-owned adapters/runtime**.
- If a consumer still relies on custom raw-fetch callbacks or side logic, the migration is only partially complete even if normalize/quality/storage/provenance are centralized.

**Decision:** `AlgoTradePlan` can cut over now for the canonical processing path, but a strict “MarketData is the sole end-to-end data platform” claim requires the remaining gaps below to be closed.

---

## 4) What still must be implemented for sole-platform status

1. **Ship MarketData-owned provider adapters** for required real sources and register them in Rust by default.
2. **Add bridge/API operations for full query/recommendation surface** (best/explain/recommend) so consumer shims do not duplicate decision logic.
3. **Provide end-to-end provider fetch command/path** in bridge/service (not only raw input ingestion).
4. **Harden storage options** for production use-cases (beyond local artifact writes).
5. **Publish multi-project compatibility matrix** (at least one non-AlgoTradePlan consumer validated in CI).
6. **Complete migration parity tests** for all datasets/use-cases required by AlgoTradePlan workflows.
7. **Keep contract-version gating mandatory** for every consumer integration pipeline.

---

## 5) Acceptance checklist for full cutover

Use this checklist as “definition of done” before declaring MarketData as the only reusable data platform:

- [ ] All required provider adapters are implemented in MarketData Rust and enabled in default runtime registry.
- [ ] Consumers no longer own raw-fetch logic for production paths (only thin transport/client wrappers remain).
- [ ] Bridge/API exposes authoritative capability/query/recommendation/explanation operations.
- [ ] All required datasets for target consumers pass end-to-end ingestion parity tests.
- [ ] Contract version checks (`doctor` + `assert-contract`) are enforced in every consumer CI.
- [ ] Storage/provenance outputs meet operational requirements (retention, reproducibility, replayability).
- [ ] At least one additional (non-AlgoTradePlan) project is integrated successfully using documented onboarding flow.
- [ ] Migration docs and rollback instructions are validated and up to date.
- [ ] Consumer repositories contain no duplicate normalization/quality/storage/provenance/query ownership.

---

## Validation snapshot in this repository

Current baseline in this repo passes:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`

This confirms current implementation health, but does **not** by itself prove full cross-project cutover completeness.
