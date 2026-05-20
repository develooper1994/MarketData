# AlgoTradePlan → MarketData: Destructive Data-Layer Migration

> **Status:** one-pass cutover – all data-layer responsibilities move to
> `develooper1994/MarketData` in this single pass.

---

## Why this migration exists

AlgoTradePlan grew an internal Python data layer
(`data/normalize.py`, `data/quality.py`, `data/storage.py`, `data/provenance.py`)
that duplicates logic already owned by the `MarketData` Rust crate.
This migration eliminates that duplication and makes `MarketData` the single
authoritative data-layer project.

After the cutover:

| Responsibility | Owner |
|---|---|
| Normalize raw payloads | **MarketData** (Rust) |
| Quality checks | **MarketData** (Rust) |
| Storage (JSONL artifacts) | **MarketData** (Rust) |
| Provenance manifests | **MarketData** (Rust) |
| Source capability metadata | **MarketData** (Rust) |
| Raw HTTP source adapters | AlgoTradePlan (Python) |
| Strategy / portfolio / risk logic | AlgoTradePlan (Python) |

---

## Pre-migration checklist

- [ ] Clone and build `develooper1994/MarketData`:

  ```bash
  git clone https://github.com/develooper1994/MarketData.git
  cd MarketData
  cargo build --release --bin market_data_bridge
  ```

- [ ] Smoke-test the bridge binary:

  ```bash
  ./target/release/market_data_bridge doctor
  # → {"status":"ok","binary":"market_data_bridge",...}
  ```

- [ ] Set the environment variable pointing AlgoTradePlan at the binary:

  ```bash
  export MARKET_DATA_BIN=/path/to/MarketData/target/release/market_data_bridge
  ```

- [ ] Run the existing AlgoTradePlan test suite **before** making any changes
  and record the baseline pass/fail counts.

---

## Step 1 – Replace `data/hub.py`

Copy `integration/algotradeplan/hub_bridge.py` from this repository to
`src/algotradeplan/data/hub.py` in AlgoTradePlan, overwriting the existing file:

```bash
cp <MarketData>/integration/algotradeplan/hub_bridge.py \
   <AlgoTradePlan>/src/algotradeplan/data/hub.py
```

The replacement file provides the exact same public API (`DataHub`, `IngestResult`)
but routes all data-processing work through the `market_data_bridge` subprocess.

---

## Step 2 – Remove the legacy Python data-processing modules

These files are now dead code; delete them:

```bash
cd <AlgoTradePlan>
git rm src/algotradeplan/data/normalize.py
git rm src/algotradeplan/data/quality.py
git rm src/algotradeplan/data/storage.py
git rm src/algotradeplan/data/provenance.py
```

The corresponding Rust implementations live in `develooper1994/MarketData`:

| Deleted Python file | Rust replacement |
|---|---|
| `data/normalize.py` | `src/normalize.rs` |
| `data/quality.py` | `src/quality.rs` |
| `data/storage.py` | `src/storage.rs` |
| `data/provenance.py` | `src/provenance.rs` |

---

## Step 3 – Remove the legacy capabilities copy

`src/algotradeplan/data/capabilities.py` is now superseded by the Rust
`src/capabilities.rs` module in `MarketData`.  The `hub_bridge.py` fetches
capability metadata from the Rust binary on first use and caches it in memory.

```bash
git rm src/algotradeplan/data/capabilities.py
```

> If other AlgoTradePlan code imports directly from `data/capabilities.py`,
> update those imports to call `hub.capability(source)` or
> `hub.sources_for(...)` instead.

---

## Step 4 – Update imports in AlgoTradePlan

Search for any remaining imports from the deleted modules and update them:

```bash
grep -r "from src.algotradeplan.data.normalize" .  # should be empty
grep -r "from src.algotradeplan.data.quality"    .  # should be empty
grep -r "from src.algotradeplan.data.storage"    .  # should be empty
grep -r "from src.algotradeplan.data.provenance" .  # should be empty
```

Typical replacements:
- `IngestResult` → import from `src.algotradeplan.data.hub`
- `CanonicalDataQualityPlugin` → no longer needed in AlgoTradePlan Python
- `InMemoryStorage`, `LocalArtifactStorage` → no longer needed
- `ManifestProvenanceTracker` → no longer needed

---

## Step 5 – Run tests

```bash
cd <AlgoTradePlan>
pytest tests/
```

Tests that exercised internal Python normalize/quality/storage should now
be deleted or updated to test the bridge integration contract.

---

## Step 6 – Verify the bridge works end-to-end

```python
import os, subprocess, json

os.environ["MARKET_DATA_BIN"] = "/path/to/market_data_bridge"

from src.algotradeplan.data.hub import DataHub

hub = DataHub()
print(hub.sources())                              # lists 24 sources
print(hub.capability("binance_futures"))          # shows capability dict
result = hub.ingest(
    source="offline_fallback",
    symbol="BTCUSDT",
    datasets=["kline"],
    store=False,
)
print(result.quality_report)                      # QualityReport(passed=True, ...)
```

---

## What to keep in AlgoTradePlan

The following files and directories should **not** be deleted:

```
src/algotradeplan/data/
    __init__.py        ← keep
    adapters/          ← keep (raw HTTP connectors – Python stays authoritative)
    etl.py             ← keep (ETL orchestration façade)
    coverage.py        ← keep (coverage table builder called by hub_bridge)
    query.py           ← keep (capability query helpers called by hub_bridge)
    hub.py             ← REPLACED by hub_bridge.py
src/algotradeplan/plugins/data/
    contracts.py       ← keep (shared dataclasses used by hub_bridge)
    interfaces.py      ← keep
```

---

## Rollback plan

If the migration causes regressions:

1. Revert `src/algotradeplan/data/hub.py` to the previous version (git).
2. Restore the deleted files from git history:
   ```bash
   git checkout HEAD~1 -- \
     src/algotradeplan/data/normalize.py \
     src/algotradeplan/data/quality.py \
     src/algotradeplan/data/storage.py \
     src/algotradeplan/data/provenance.py \
     src/algotradeplan/data/capabilities.py
   ```
3. Unset `MARKET_DATA_BIN`.

---

## Architecture after migration

```
AlgoTradePlan (Python)
│
├── data/adapters/         ← raw HTTP calls to Binance, Yahoo, TEFAS, etc.
├── data/hub.py            ← thin bridge shim (delegates to MarketData)
│       │
│       │  subprocess (stdin JSON → stdout JSON)
│       ▼
│   market_data_bridge (Rust binary)
│       ├── capabilities  → sources / dataset status / rankings
│       ├── ingest        → normalize + quality + storage + provenance
│       └── query-sources-for → filtered source list
│
├── strategies/            ← strategy logic unchanged
├── portfolio/             ← portfolio logic unchanged
└── backtest/              ← backtest logic unchanged
```

---

## Rust migration roadmap

Phase 1 (done): bridge CLI covers normalize / quality / storage / provenance /
capabilities.

Phase 2: Move the raw HTTP adapter logic (currently in `data/adapters/`) into
Rust async tasks using `reqwest`.  AlgoTradePlan then simply calls the bridge
with no pre-fetching step.

Phase 3: Expose the bridge as a gRPC microservice (`tonic`) so AlgoTradePlan
can run as a separate process and call it over the network.

Phase 4: Full Rust rewrite of the hot path (indicator computation, backtest
core) for sub-millisecond latency.
