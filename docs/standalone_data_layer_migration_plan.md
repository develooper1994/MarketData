# MarketData — Architecture and Ownership Model

`MarketData` is the **single authoritative data-layer platform** for every project
in this ecosystem.  Consumer projects (AlgoTradePlan and any future project) keep
at most a thin bridge shim; they own no data logic.

For current readiness status and actionable TODOs see [`docs/STATUS.md`](./STATUS.md).

---

## Target architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                       MarketData (Rust crate)                       │
│                                                                      │
│  capabilities (24-source registry)   query (filter/rank/recommend)  │
│  contracts (DataRequest/Record/…)    normalize (9 dataset types)    │
│  hub (DataHub orchestration)         quality (validation checks)    │
│  storage (in-memory / local JSONL)   provenance (manifest tracker)  │
│  etl (fluent façade)                 SourceAdapterRegistry + trait  │
└─────────────────────────────────────────────────────────────────────┘
          ▲ Rust crate dep        ▲ bridge subprocess      ▲ gRPC (Phase 3)
          │                       │                         │
   Rust projects          AlgoTradePlan / Python    Future remote clients
                          (thin shim only)
```

---

## Ownership model

| Responsibility | Owner |
|---|---|
| Provider/adapter registry | **MarketData** `SourceAdapterRegistry` |
| Capability/query/recommendation logic | **MarketData** `src/capabilities.rs` + `src/query.rs` |
| Ingestion orchestration | **MarketData** `DataHub` |
| Normalization (9 dataset types) | **MarketData** `src/normalize.rs` |
| Quality validation | **MarketData** `src/quality.rs` |
| Storage and artifact persistence | **MarketData** `src/storage.rs` |
| Provenance manifests | **MarketData** `src/provenance.rs` |
| Public API / CLI / bridge contracts | **MarketData** (shim only in consumers) |
| Dataset and provider metadata | **MarketData** |
| Data-layer tests and fixtures | **MarketData** |

---

## What consumer projects may keep

Only a thin compatibility facade:

```
AlgoTradePlan/src/algotradeplan/data/
    __init__.py        ← keep (package marker)
    hub.py             ← REPLACED by hub_bridge.py (no data logic — delegates everything)
AlgoTradePlan/src/algotradeplan/plugins/data/
    contracts.py       ← keep only as pure DTOs (no logic)
    interfaces.py      ← keep only as abstract base classes
```

Any file that contains its own normalization, quality, storage, provenance, or
recommendation logic **must be deleted** from the consumer project.

---

## Integration patterns

### Pattern 1 — Rust crate

```toml
[dependencies]
market_data = { path = "../MarketData" }
```

```rust
use market_data::prelude::*;

let mut hub = DataHub::with_components(
    Box::new(InMemoryStorage::default()),
    ManifestProvenanceTracker::new(None::<&str>),
    SourceAdapterRegistry::default(),
);
let result = hub.ingest_from_raw("offline", "BTCUSDT", vec!["kline".into()], payload, false)?;
```

### Pattern 2 — Subprocess bridge (Python / polyglot)

```bash
cargo build --release --bin market_data_bridge
export MARKET_DATA_BIN="$PWD/target/release/market_data_bridge"
```

```python
import json, os, subprocess

def run_bridge(command, *, stdin=""):
    result = subprocess.run(
        [os.environ["MARKET_DATA_BIN"], *command],
        input=stdin, text=True, capture_output=True, check=True,
    )
    return json.loads(result.stdout)

run_bridge(["doctor"])          # health check
run_bridge(["sources"])         # list all sources
run_bridge(["capabilities"])    # full capability metadata
```

See [`integration/algotradeplan/hub_bridge.py`](../integration/algotradeplan/hub_bridge.py) for the
full reference wrapper.

### Pattern 3 — gRPC service (Phase 3, planned)

Same contract fields over the network via `tonic`.  Pin to `contract_version: 1`.

---

## AlgoTradePlan cutover steps

1. Build bridge binary: `cargo build --release --bin market_data_bridge`
2. Set `MARKET_DATA_BIN` in the AlgoTradePlan runtime environment
3. Copy `integration/algotradeplan/hub_bridge.py` → `AlgoTradePlan/src/algotradeplan/data/hub.py`
4. Run smoke tests via the shim (`hub.sources()`, `hub.ingest(...)`)
5. Delete the internal Python data layer (see
   [`integration/algotradeplan/migration_cutover.md`](../integration/algotradeplan/migration_cutover.md)
   for the exact `git rm` commands)
6. Search and fix any remaining imports from deleted modules
7. Run full AlgoTradePlan test suite; assert it passes through the shim

**Acceptance criteria for cutover:**
- No AlgoTradePlan code imports any deleted internal data module
- `hub.ingest(...)` returns valid `IngestResult` with quality/provenance
- All AlgoTradePlan tests pass
- `market_data_bridge assert-contract --expected 1` succeeds in CI

---

## Validation requirements

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

All must pass.  Every consumer CI must also call `market_data_bridge doctor` and
assert `contract_version: 1` on startup.

---

## Reuse model for future projects

Any future project follows the same three patterns above.  Provider logic,
normalization, quality policy, and provenance auditing stay centralized in
`MarketData` and never fragment back into individual consumer projects.

See [`docs/new_project_onboarding.md`](./new_project_onboarding.md) for a
step-by-step guide for new consumers.
