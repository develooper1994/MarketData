# MarketData Standalone Data-Layer Migration Plan

This document defines the target architecture and destructive cutover plan for
making `develooper1994/MarketData` the standalone, reusable, authoritative data
layer for `develooper1994/AlgoTradePlan` and future projects.

## 1) Current intended boundary (as-is intent)

Current intent is to keep domain logic in `AlgoTradePlan` while centralizing all
data responsibilities in `MarketData`:

- **AlgoTradePlan owns**: strategy, portfolio, risk, backtest orchestration, and
  application workflows.
- **MarketData owns**: source metadata, data-source selection/query behavior,
  ingestion pipeline, normalization, quality validation, storage, provenance, and
  integration surfaces used by clients.

Any data logic that remains in `AlgoTradePlan` after cutover is compatibility-only
and must not become a second source of truth.

## 2) End-state architecture (target)

`MarketData` becomes the only data-layer authority, exposed through reusable
surfaces:

1. **Rust library API** (`market_data` crate) for in-process use by Rust projects.
2. **Bridge CLI** (`market_data_bridge`) for language-agnostic subprocess use.
3. **Compatibility shim(s)** in client repos (for example `AlgoTradePlan/data/hub.py`)
   that only translate client calls to MarketData commands/contracts.

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

## 3) Responsibilities that must move from AlgoTradePlan to MarketData

The following must be migrated or deleted from `AlgoTradePlan` ownership:

1. **Source/provider adapters**
   - Remove Python-owned adapters and registry logic as authoritative behavior.
   - Use MarketData adapter-facing surfaces and provider metadata.
2. **Capability/query/recommendation logic**
   - Remove local capability maps, coverage tables, and source recommendation helpers.
   - Query via MarketData `capabilities`, `sources`, and `query-sources-for`.
3. **Ingestion orchestration**
   - Route ingestion through `market_data_bridge ingest` (or Rust API where applicable).
4. **Normalization**
   - Remove local normalize modules; rely on MarketData normalizers.
5. **Quality checks**
   - Remove local quality plugins/checkers; rely on MarketData quality pipeline.
6. **Storage and provenance**
   - Remove local storage/provenance implementations; rely on MarketData storage/provenance.
7. **Public API/CLI/bridge/client surfaces**
   - Keep only compatibility wrapper methods in clients; behavior and contracts are owned by MarketData.
8. **Dataset and provider metadata**
   - Keep metadata canonical in MarketData; clients consume read-only outputs.
9. **Tests and validation fixtures**
   - Move data-layer parity and fixture validation into MarketData test suite.

## 4) What may remain in AlgoTradePlan (thin shim only)

Only a compatibility facade may remain in `AlgoTradePlan`:

- `src/algotradeplan/data/hub.py` (or equivalent) as a **thin client shim**.
- Shared client-side data contracts that are purely transport/domain DTOs, if needed.

The shim is acceptable only when it:

- contains no independent normalization/quality/storage/provenance logic,
- contains no independent capability ranking/recommendation rules,
- delegates all data decisions to MarketData,
- can be replaced by a direct client (`CLI`, service, or future SDK) without semantic change.

## 5) Destructive cutover migration sequence

This is a one-pass cutover plan with explicit rollback points.

### Phase A — Pre-cutover readiness

Acceptance criteria:

- MarketData bridge supports required datasets/providers used by AlgoTradePlan.
- `doctor`, `capabilities`, `sources`, `query-sources-for`, and `ingest` commands are stable.
- MarketData tests (unit/integration/bridge contract) pass.
- Baseline AlgoTradePlan tests are captured before destructive changes.

Rollback:

- No deletion yet; stop and remediate missing datasets/providers in MarketData first.

### Phase B — Compatibility cut-in

Actions:

1. Replace `AlgoTradePlan` data hub entrypoint with MarketData-backed shim.
2. Wire runtime to `MARKET_DATA_BIN` (or chosen deployment contract).
3. Run smoke scenarios for `sources`, `capability`, `sources_for`, and `ingest`.

Acceptance criteria:

- Existing AlgoTradePlan call sites work through shim without API-level breakage.
- End-to-end ingest returns records, quality report, storage receipts/provenance as expected.

Rollback:

- Revert shim replacement and environment wiring.

### Phase C — Destructive removal of internal data layer

Actions:

1. Delete legacy data modules from AlgoTradePlan:
   - provider adapters/registry owned by old data layer,
   - capability/query/coverage modules,
   - normalize, quality, storage, provenance modules.
2. Update imports/call sites to use shim only.
3. Remove dead tests tied to deleted internal implementations and replace with integration contract tests.

Acceptance criteria:

- No production code imports deleted internal data modules.
- Data-related behavior in AlgoTradePlan is reachable only through MarketData-backed surfaces.
- Full AlgoTradePlan test suite passes with MarketData connected.

Rollback:

- Restore deleted files from VCS and revert import rewrites.
- Re-enable prior local data path if critical regression appears.

### Phase D — Stabilization and multi-project hardening

Actions:

1. Promote MarketData contracts as versioned integration contract.
2. Add fixture-driven parity/regression tests in MarketData for client-critical datasets/providers.
3. Document onboarding pattern for additional projects (CLI or future service client).

Acceptance criteria:

- At least one non-AlgoTradePlan consumer can integrate via documented contract.
- Contract/versioning policy is documented and tested in MarketData CI.

Implementation status:

- `BRIDGE_CONTRACT_VERSION` constant added to `market_data_bridge`; reported in `doctor` output under `contract_version` and `bridge_contract.contract_version`.
- Fixture-driven bridge parity tests added for all 9 dataset types (`kline`, `tick`, `trade`, `orderbook`, `funding`, `macro`, `news`, `fundamentals`, `corporate_actions`) in `tests/bridge_cli_tests.rs`.
- Onboarding documentation for any project published at [`docs/new_project_onboarding.md`](../docs/new_project_onboarding.md).

Rollback:

- Keep shim pinned to previous MarketData version while fixing contract regressions.

## 6) Practical acceptance criteria for deleting AlgoTradePlan data layer

`AlgoTradePlan` may delete its internal data layer only when all are true:

1. **Ownership**: every data-layer responsibility listed in this document is implemented in MarketData.
2. **No dual authority**: no duplicate capability/query/normalization/quality/storage logic remains active in AlgoTradePlan.
3. **Contract parity**: required AlgoTradePlan data workflows pass through MarketData shim/API without behavior loss.
4. **Operational readiness**: environment/config/deployment instructions for MarketData dependency are documented and tested.
5. **Rollback readiness**: a tested rollback path exists for at least one release window.

## Reuse model for future projects

Any future project should consume MarketData using one of these patterns:

- **Rust-native**: import `market_data` crate directly.
- **Polyglot/local**: invoke `market_data_bridge` CLI as subprocess.
- **Polyglot/remote (planned)**: call future service endpoint while keeping same data contracts.

This keeps provider logic, normalization policy, quality policy, and provenance
auditing centralized in one place and prevents data-layer re-fragmentation.
