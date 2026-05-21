# MarketData — Standalone Data-Layer Architecture and Migration Plan

This is the **single authoritative document** that defines:

- the target architecture of `develooper1994/MarketData` as an exclusive,
  reusable data-layer platform;
- the full set of responsibilities that must transfer from `AlgoTradePlan`
  (or any other consumer) to `MarketData`;
- how adapters, metadata, client surfaces, and query logic must be structured
  inside `MarketData`;
- exactly what (if anything) may remain in `AlgoTradePlan`;
- the destructive one-pass migration sequence with acceptance criteria; and
- documentation and validation requirements.

`MarketData` is not a helper library for one project.  It is the **standalone,
reusable data-layer platform** for every project in this ecosystem.

---

## 1) Current boundary intent

Domain logic is kept in each consumer project while all data responsibilities
are centralized in `MarketData`:

- **AlgoTradePlan owns**: strategy, portfolio, risk, backtest orchestration,
  and application workflows.
- **MarketData owns**: provider/adapter registry, source capability and
  recommendation logic, ingestion orchestration, normalization, quality
  validation, storage, provenance manifests, and every public integration
  surface used by clients (Rust API, bridge CLI, future gRPC service).

Any data logic that remains in `AlgoTradePlan` after cutover is
compatibility-only and must not become a second source of truth.

---

## 2) Target architecture

### High-level structure

```
┌─────────────────────────────────────────────────────────────────────┐
│                       MarketData (Rust crate)                       │
│                                                                     │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────────────────┐   │
│  │  capabilities│  │    query     │  │      contracts         │   │
│  │  (24-source  │  │  (sources_   │  │  DataRequest /         │   │
│  │  registry +  │  │  for, best_  │  │  DataRecord /          │   │
│  │  metadata)   │  │  sources_for │  │  QualityReport /       │   │
│  │              │  │  etc.)       │  │  StorageReceipt /      │   │
│  └──────────────┘  └──────────────┘  │  ProvenanceRecord /   │   │
│                                      │  IngestResult)         │   │
│  ┌──────────────────────────────┐    └────────────────────────┘   │
│  │             hub              │                                  │
│  │  (DataHub: normalize →       │  ┌──────────┐  ┌────────────┐  │
│  │   quality → storage →        │  │ normalize│  │  quality   │  │
│  │   provenance orchestration)  │  │ (9 types)│  │ (checks)   │  │
│  └──────────────────────────────┘  └──────────┘  └────────────┘  │
│                                                                     │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐ │
│  │   storage    │  │  provenance  │  │    SourceAdapterRegistry  │ │
│  │ (in-memory / │  │  (manifest   │  │    + RawSourceAdapter     │ │
│  │  local JSONL)│  │  tracker)    │  │    trait                  │ │
│  └──────────────┘  └──────────────┘  └──────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
            ▲                    ▲                    ▲
            │ Rust crate dep     │ bridge subprocess  │ gRPC (Phase 3)
            │                    │                    │
    ┌───────────────┐  ┌────────────────────┐  ┌──────────────────┐
    │ Rust projects │  │ AlgoTradePlan /    │  │ Future remote    │
    │               │  │ Python projects /  │  │ clients          │
    │               │  │ any polyglot       │  │                  │
    └───────────────┘  └────────────────────┘  └──────────────────┘
```

### Public surfaces (all owned by MarketData)

| Surface | Who uses it | Current status |
|---|---|---|
| `market_data` Rust crate | Rust projects | Live |
| `market_data_bridge` CLI | Any language via subprocess | Live |
| `integration/algotradeplan/hub_bridge.py` | AlgoTradePlan thin shim | Live |
| gRPC microservice (`tonic`) | Remote/network clients | Phase 3 (planned) |

### Target ownership model

| Capability area | Final owner |
|---|---|
| Source/provider adapters and adapter registry | `MarketData` |
| Capability/query/recommendation logic | `MarketData` |
| Ingestion orchestration | `MarketData` |
| Normalization and canonical dataset mapping | `MarketData` |
| Quality checks and policy enforcement | `MarketData` |
| Storage, artifact persistence, and provenance manifests | `MarketData` |
| Public API/CLI/bridge/client contracts | `MarketData` (shim only in clients) |
| Dataset and provider metadata | `MarketData` |
| Data-layer tests and validation fixtures | `MarketData` |

---

## 3) How adapters, metadata, client surfaces, and query logic must live in MarketData

### 3a) Adapter/provider architecture

All provider and adapter logic lives in `MarketData`, never in a consumer
project:

- **`RawSourceAdapter` trait** (`src/hub.rs`) is the single extension point.
  Every data provider implements this trait inside the `MarketData` crate.
- **`SourceAdapterRegistry`** (`src/hub.rs`) is the runtime registry that
  maps provider names to adapter implementations.  Consumers never maintain
  their own registry.
- **`capabilities` module** (`src/capabilities.rs`) holds the authoritative
  24-source capability map.  It is the only place that records which datasets,
  asset classes, realtime flags, API key requirements, and quality levels
  each provider supports.
- **Adapter fixture data** lives in the `MarketData` test suite, not in
  consumer repos.
- **Adding a new provider**: implement `RawSourceAdapter`, register it in
  `SourceAdapterRegistry::default()`, add its `SourceCapability` entry to
  `all_capabilities()`, and add fixture-driven contract tests in
  `tests/bridge_cli_tests.rs`.

### 3b) Metadata architecture

All dataset and provider metadata is owned and served by `MarketData`:

- `src/capabilities.rs` — source capability records (24 sources): asset
  classes, supported datasets, realtime/history flags, API key policy, rate
  limit notes, quality level, implementation status.
- `src/contracts.rs` — canonical data contracts (`DataRequest`, `DataRecord`,
  `QualityReport`, `StorageReceipt`, `ProvenanceRecord`, `IngestResult`).
  These structs are the stable, versioned wire format shared with all clients.
- `BRIDGE_CONTRACT_VERSION` (`src/bin/market_data_bridge.rs`) — a monotonic
  integer that clients pin to detect breaking changes.  Clients must call
  `doctor` on startup and assert the expected version.
- `src/normalize.rs` — canonical dataset-type aliases
  (e.g. `"ohlcv"` → `"kline"`) so consumers never maintain their own alias
  tables.

Consumers must not duplicate any of the above.  They read metadata from
`capabilities`, `sources`, `query-sources-for`, or `doctor` and treat it
as read-only.

### 3c) Client surface architecture

MarketData exposes exactly three client integration patterns.  Consumer
projects choose one; they must not implement any data logic themselves:

#### Pattern 1 — Rust crate (Rust consumers)

```toml
# Cargo.toml of the consuming project
[dependencies]
market_data = { path = "../MarketData" }
```

```rust
use market_data::{DataHub, InMemoryStorage, ManifestProvenanceTracker, SourceAdapterRegistry};

let mut hub = DataHub::with_components(
    Box::new(InMemoryStorage::default()),
    ManifestProvenanceTracker::new(None::<&str>),
    SourceAdapterRegistry::default(),
);
let result = hub.ingest_from_raw("offline", "BTCUSDT", vec!["kline".into()], payload, false)?;
```

The consumer calls public API only; it never reaches into `normalize`,
`quality`, `storage`, or `provenance` modules directly.

#### Pattern 2 — Subprocess bridge (Python / polyglot consumers)

The bridge binary is built once from `MarketData` and pointed to by the
`MARKET_DATA_BIN` environment variable in the consumer's runtime:

```bash
cargo build --release --bin market_data_bridge
export MARKET_DATA_BIN="$PWD/target/release/market_data_bridge"
```

The consumer wraps the binary in a thin subprocess helper (see
`integration/algotradeplan/hub_bridge.py` as the reference implementation).
The helper must:

- accept `MARKET_DATA_BIN` from the environment (never hard-code a path),
- call `doctor` once on startup and assert the expected `contract_version`,
- pass raw JSON in via stdin and parse JSON out from stdout,
- expose only the five standard commands (`doctor`, `sources`, `capabilities`,
  `query-sources-for`, `ingest`) — no extra data logic.

The wrapper is **not** a data layer.  It is a thin protocol adapter.

#### Pattern 3 — gRPC service (planned, Phase 3)

A future release will wrap the same bridge logic behind a `tonic` gRPC server.
Clients will call the same five operations over the network using the same
contract structs serialised as Protobuf.  Clients should pin to
`contract_version: 1` now; the gRPC service will maintain the same field names.

### 3d) Query logic architecture

All query and recommendation logic lives in `src/query.rs`:

- `sources_for(dataset, asset_class, require_live)` — returns sorted source
  names that support the requested dataset + optional filters.
- `best_sources_for(dataset)` — returns the top-ranked source for a dataset.
- `dataset_status_for_source(source, dataset)` — returns live/delayed/eod/
  unsupported status.
- `asset_status_for_source(source, asset_class)` — returns asset-class support
  status for a source.
- `available_datasets(source)` — returns the implemented dataset list.
- `source_summary(source)` — returns a one-line human-readable summary.

These functions are the authoritative home for all source-selection logic.
Consumer projects must not implement their own `recommend_sources`,
`best_sources_for`, `explain_source`, or `explain_dataset` logic.  They must
call the bridge `query-sources-for` command (or the Rust functions directly).

---

## 4) Responsibilities that must transfer from AlgoTradePlan to MarketData

The following must be migrated to `MarketData` or deleted outright from
`AlgoTradePlan`:

| Responsibility | Action in AlgoTradePlan | Owner after cutover |
|---|---|---|
| Provider/adapter registry | Delete | `MarketData` `SourceAdapterRegistry` |
| `data/capabilities.py` | Delete | `MarketData` `src/capabilities.rs` |
| `data/query.py` | Delete | `MarketData` `src/query.rs` + bridge |
| `data/coverage.py` | Delete | `MarketData` bridge `capabilities` command |
| `data/normalize.py` | Delete | `MarketData` `src/normalize.rs` |
| `data/quality.py` | Delete | `MarketData` `src/quality.rs` |
| `data/storage.py` | Delete | `MarketData` `src/storage.rs` |
| `data/provenance.py` | Delete | `MarketData` `src/provenance.rs` |
| `data/adapters/` directory | Delete | `MarketData` adapter-facing surface |
| `data/hub.py` (logic) | Replace with shim | `integration/algotradeplan/hub_bridge.py` |
| Source recommendation helpers | Delete | `MarketData` `src/query.rs` |
| Dataset fixture files | Move to MarketData | `MarketData` test fixtures |
| Data-layer unit/integration tests | Move/rewrite | `MarketData` `tests/` |

---

## 5) What may remain in AlgoTradePlan (thin shim only)

Only a compatibility facade may remain in `AlgoTradePlan`:

```
src/algotradeplan/data/
    __init__.py        ← keep (package marker)
    hub.py             ← REPLACED by hub_bridge.py (thin shim, no data logic)
src/algotradeplan/plugins/data/
    contracts.py       ← keep only if needed as pure DTOs with no logic
    interfaces.py      ← keep only if needed as abstract base classes
```

The shim (`hub.py`) is acceptable **only** when it:

- contains no independent normalization, quality, storage, or provenance logic;
- contains no independent capability ranking or recommendation rules;
- delegates every data decision to `MarketData` via subprocess or Rust API;
- can be replaced by a direct client (CLI, service, or future SDK) without
  any semantic change to callers.

Any file that does not meet these criteria must be deleted.

---

## 6) Destructive one-pass migration sequence

This is a one-pass cutover plan.  Each phase has explicit acceptance criteria
and a rollback path.  Do not proceed to the next phase until all criteria of
the current phase are met.

### Phase A — Pre-cutover readiness

Actions:

1. Verify `cargo build --release --bin market_data_bridge` succeeds.
2. Verify `cargo test` passes all 34 tests.
3. Run `market_data_bridge doctor` and confirm `status: ok` and expected
   `contract_version`.
4. Record the baseline AlgoTradePlan test pass/fail counts before any changes.

Acceptance criteria:

- MarketData bridge supports every dataset and provider used by AlgoTradePlan.
- `doctor`, `capabilities`, `sources`, `query-sources-for`, and `ingest`
  commands are stable and return valid JSON.
- All MarketData unit, integration, and bridge-contract tests pass.
- Baseline AlgoTradePlan test snapshot captured.

Rollback:

- No deletion has occurred; stop here and fix any missing dataset/provider
  coverage in MarketData.

### Phase B — Compatibility cut-in

Actions:

1. Copy `integration/algotradeplan/hub_bridge.py` to
   `AlgoTradePlan/src/algotradeplan/data/hub.py` (overwrite existing file).
2. Set `MARKET_DATA_BIN` in the runtime environment to the compiled bridge path.
3. Run smoke tests: `hub.sources()`, `hub.capability(src)`,
   `hub.sources_for(dataset)`, `hub.ingest(...)`.

Acceptance criteria:

- Existing AlgoTradePlan call sites work through the shim with no API-level
  breakage.
- End-to-end ingest returns valid `IngestResult` with records, quality report,
  storage receipts, and provenance.

Rollback:

- Revert `hub.py` to the previous version from git.
- Unset `MARKET_DATA_BIN`.

### Phase C — Destructive removal of internal data layer

Actions:

1. Delete the following from AlgoTradePlan:
   ```bash
   git rm src/algotradeplan/data/normalize.py
   git rm src/algotradeplan/data/quality.py
   git rm src/algotradeplan/data/storage.py
   git rm src/algotradeplan/data/provenance.py
   git rm src/algotradeplan/data/capabilities.py
   git rm src/algotradeplan/data/query.py
   git rm src/algotradeplan/data/coverage.py
   git rm -r src/algotradeplan/data/adapters
   ```
2. Search for and fix any remaining imports of the deleted modules:
   ```bash
   grep -r "from.*data\.normalize"    src/
   grep -r "from.*data\.quality"      src/
   grep -r "from.*data\.storage"      src/
   grep -r "from.*data\.provenance"   src/
   grep -r "from.*data\.capabilities" src/
   grep -r "from.*data\.query"        src/
   grep -r "from.*data\.coverage"     src/
   ```
   All results must be empty.
3. Delete dead tests tied to the deleted internal implementations and replace
   with bridge integration contract tests.

Acceptance criteria:

- No production code imports any deleted internal data module.
- All data-related behavior in AlgoTradePlan is reachable only through the
  MarketData-backed shim.
- Full AlgoTradePlan test suite passes with MarketData connected.

Rollback:

```bash
git checkout HEAD~1 -- \
  src/algotradeplan/data/normalize.py \
  src/algotradeplan/data/quality.py \
  src/algotradeplan/data/storage.py \
  src/algotradeplan/data/provenance.py \
  src/algotradeplan/data/capabilities.py \
  src/algotradeplan/data/query.py \
  src/algotradeplan/data/coverage.py
git checkout HEAD~1 -- src/algotradeplan/data/adapters
```

### Phase D — Stabilization and multi-project hardening

Actions:

1. Promote MarketData contracts as a versioned integration contract (already
   done: `BRIDGE_CONTRACT_VERSION = "1"`).
2. Ensure fixture-driven parity and regression tests cover all 9 dataset types
   in `tests/bridge_cli_tests.rs` (already done).
3. Publish onboarding documentation for additional projects at
   `docs/new_project_onboarding.md` (already done).
4. Tag the MarketData repo with `marketdata-cutover-v1` and the AlgoTradePlan
   repo with `before-marketdata-cutover` before destructive deletion.

Acceptance criteria:

- At least one non-AlgoTradePlan consumer can integrate via the documented
  subprocess or Rust-crate patterns.
- Contract versioning policy is documented and validated in MarketData CI.
- Both rollback tags exist in their respective repos.

Implementation status:

- `BRIDGE_CONTRACT_VERSION` constant defined in `src/bin/market_data_bridge.rs`
  and reported in `doctor` output under `contract_version` and
  `bridge_contract.contract_version`.
- Fixture-driven bridge parity tests cover all 9 dataset types
  (`kline`, `tick`, `trade`, `orderbook`, `funding`, `macro`, `news`,
  `fundamentals`, `corporate_actions`) in `tests/bridge_cli_tests.rs`.
- New-project onboarding guide published at `docs/new_project_onboarding.md`.

Rollback:

- Keep the consumer shim pinned to a known `contract_version` while fixing any
  contract regression.

---

## 7) Practical acceptance criteria for deleting AlgoTradePlan's data layer

`AlgoTradePlan` may delete its internal data layer only when **all** of the
following are true:

1. **Ownership**: every data-layer responsibility in this document is
   implemented in `MarketData` and no equivalent implementation exists in
   `AlgoTradePlan`.
2. **No dual authority**: no duplicate capability, query, normalization,
   quality, storage, or provenance logic is active in `AlgoTradePlan`.
3. **Contract parity**: all `AlgoTradePlan` data workflows that previously
   ran through the internal data layer now run through the MarketData shim or
   API and produce identical results.
4. **Operational readiness**: environment/config/deployment instructions for
   the `MarketData` dependency are documented, tested, and available to every
   developer.
5. **Rollback readiness**: a tested rollback path exists and is documented for
   at least one release window after the destructive deletion.

---

## 8) Documentation requirements

The following documents must exist and stay current:

| Document | Location | Owner | What it covers |
|---|---|---|---|
| This migration plan | `docs/standalone_data_layer_migration_plan.md` | MarketData | Architecture, responsibilities, migration sequence, acceptance criteria |
| Integration guide | `docs/algotradeplan_integration.md` | MarketData | Architecture diagram, contract mapping, CLI reference, cutover steps |
| New-project onboarding | `docs/new_project_onboarding.md` | MarketData | How any project integrates (Rust, subprocess, future gRPC) |
| Cutover guide | `integration/algotradeplan/migration_cutover.md` | MarketData | Step-by-step destructive migration instructions for AlgoTradePlan |
| Bridge shim | `integration/algotradeplan/hub_bridge.py` | MarketData | Drop-in `hub.py` replacement; reference subprocess wrapper |
| Usage example | `integration/algotradeplan/datahub_bridge_example.py` | MarketData | Standalone end-to-end usage example |

Each document must be updated whenever:

- a new dataset type is added to `MarketData`,
- a new source is added to the capabilities registry,
- the bridge contract version is incremented,
- a new integration pattern is added (e.g. gRPC).

---

## 9) Validation and testing requirements

### 9a) MarketData CI must pass before any consumer cutover

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

All tests must pass with zero warnings promoted to errors.

### 9b) Required test coverage in MarketData

| Test file | Coverage requirement |
|---|---|
| `tests/bridge_cli_tests.rs` | All 9 dataset types via `ingest`; `doctor`, `sources`, `capabilities`, `query-sources-for` commands; missing-dataset error surface |
| `tests/data_hub_tests.rs` | `DataHub::ingest_from_raw` round-trip; quality detection; ETL adapter integration |
| Module-level unit tests (`#[cfg(test)]`) | `normalize`, `quality`, `storage`, `provenance`, `query`, `capabilities` internal logic |

### 9c) Contract validation on every consumer integration

Every consumer project must call `market_data_bridge doctor` in its CI/test
setup and assert:

```json
{ "status": "ok", "contract_version": "1" }
```

Additionally, consumers should enforce compatibility with:

```bash
market_data_bridge assert-contract --expected 1
```

If `contract_version` changes, the consumer's integration test suite must be
updated before the consumer can ship.

### 9d) Parity validation after AlgoTradePlan cutover

After Phase C (destructive deletion), run this validation matrix in
AlgoTradePlan:

- `hub.sources()` returns a non-empty list matching the MarketData source
  registry.
- `hub.capability(source)` returns a valid capability dict for every source
  previously used by AlgoTradePlan strategies.
- `hub.ingest(source, symbol, datasets, store=False)` returns an
  `IngestResult` with `quality_report.passed == True` for at least one
  real or mock/offline dataset per strategy used in backtest.
- No test or strategy code imports from any deleted module
  (`data.normalize`, `data.quality`, `data.storage`, `data.provenance`,
  `data.capabilities`, `data.query`, `data.coverage`).

### 9e) Rollback verification

Before tagging `before-marketdata-cutover`:

1. Confirm the rollback commands in Phase C restore a working AlgoTradePlan
   test suite against the old internal data layer (without MarketData).
2. Confirm `MARKET_DATA_BIN` unset + reverted `hub.py` returns AlgoTradePlan
   to a fully operational prior state.

---

## 10) Reuse model for future projects

Any future project consumes `MarketData` using one of three patterns:

- **Rust-native**: add the `market_data` crate as a dependency and call the
  library API directly (see `docs/new_project_onboarding.md` § Option 1).
- **Polyglot/local**: invoke `market_data_bridge` as a subprocess (see
  `docs/new_project_onboarding.md` § Option 2).
- **Polyglot/remote** (Phase 3): call the gRPC microservice over the network
  with the same contract fields (see `docs/new_project_onboarding.md`
  § Option 3).

This ensures provider logic, normalization policy, quality policy, and
provenance auditing stay centralised in `MarketData` and never fragment back
into individual consumer projects.
