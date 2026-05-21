# Using MarketData From a New Project

`develooper1994/MarketData` is the standalone, reusable data layer for market
data ingestion, normalization, quality validation, storage, and provenance.
This guide shows how any project (Rust, Python, or other) can consume it.

---

## What MarketData provides

| Capability | How it is exposed |
|---|---|
| 24-source registry with capability metadata | `capabilities` CLI command / Rust `all_capabilities()` |
| Source discovery by dataset / asset class | `query-sources-for` CLI command / Rust `sources_for(...)` |
| Ranked source selection | `query-best-sources` CLI command / Rust `best_sources_for(...)` |
| Source and dataset explain helpers | `query-source-summary` + `query-dataset-summary` CLI commands |
| Use-case recommendations | `recommend-sources` + `supported-use-cases` CLI commands |
| Ingestion pipeline (normalize → quality → store → provenance) | `ingest` CLI command / Rust `DataHub::ingest_from_raw` |
| 9 dataset types supported | `kline` `tick` `trade` `orderbook` `funding` `macro` `news` `fundamentals` `corporate_actions` |
| Environment health check | `doctor` CLI command |

---

## Contract version

Run `market_data_bridge doctor` to verify the contract version your binary implements:

```json
{
  "contract_version": "1",
  "bridge_contract": {
    "contract_version": "1",
    "raw_datasets": true,
    "storage_receipts": true,
    "provenance": true,
    "capabilities": true,
    "query_sources_for": true
  }
}
```

Pin your client to a known `contract_version` to detect breaking changes early.

For CI/startup fail-fast enforcement, use:

```bash
market_data_bridge assert-contract --expected 1
```

The command exits non-zero when the bridge contract differs from the pinned version.

---

## Option 1 - Rust-native (recommended for Rust projects)

Add the crate as a path or git dependency in your `Cargo.toml`:

```toml
[dependencies]
market_data = { path = "../MarketData" }
# or, once published:
# market_data = "0.1"
```

Use the library directly:

```rust
use market_data::{DataHub, InMemoryStorage, ManifestProvenanceTracker, SourceAdapterRegistry};
use std::collections::HashMap;

let mut hub = DataHub::with_components(
    Box::new(InMemoryStorage::default()),
    ManifestProvenanceTracker::new(None::<&str>),
    SourceAdapterRegistry::default(),
);

let result = hub.ingest_from_raw(
    "offline",
    "BTCUSDT",
    vec!["kline".to_string()],
    HashMap::from([("kline".to_string(), serde_json::json!([[1716200000000_i64,"10","11","9","10.5","42"]]))]),
    true,
)?;
println!("{}", result.quality_report.passed);
```

---

## Option 2 - Subprocess bridge (polyglot / Python / any language)

For the canonical command list and common flows, see:
[`README.md`](../README.md#bridge-cli-quick-use)

### Step 1 - Build the binary

```bash
cd /path/to/MarketData
cargo build --release --bin market_data_bridge
export MARKET_DATA_BIN="$PWD/target/release/market_data_bridge"
```

### Step 2 - Verify setup

```bash
$MARKET_DATA_BIN doctor
# → {"status":"ok","contract_version":"1",...}
```

### Step 3 - Query available sources

```bash
$MARKET_DATA_BIN sources
$MARKET_DATA_BIN capabilities
$MARKET_DATA_BIN query-sources-for --dataset kline --asset-class crypto_spot
$MARKET_DATA_BIN query-best-sources --dataset kline --asset-class crypto_spot --limit 5
$MARKET_DATA_BIN query-source-summary --source binance_futures
$MARKET_DATA_BIN query-dataset-summary --dataset kline
$MARKET_DATA_BIN supported-use-cases
$MARKET_DATA_BIN recommend-sources --use-case crypto_backtest --limit 5
```

### Step 4 - Ingest data (pipe JSON in, receive JSON result)

```bash
printf '{"kline":[[1716200000000,100,110,90,105,1000]]}' | \
  $MARKET_DATA_BIN ingest \
    --source binance_futures \
    --symbol BTCUSDT \
    --datasets kline \
    --asset-type crypto_spot \
    --store \
    --record-root ./artifacts/records \
    --manifest-root ./artifacts/manifests
```

The command returns a JSON `IngestResult` object:

```json
{
  "source": "binance_futures",
  "symbol": "BTCUSDT",
  "dataset_coverage": {"kline": 1},
  "records": [{"key": "binance_futures:kline:BTCUSDT:1716200000000:1", "domain": "market", ...}],
  "quality_report": {"passed": true, "issues": []},
  "storage_receipts": [{"location": "./artifacts/records/..."}],
  "provenance": {"source_plugin_id": "binance_futures", ...}
}
```

### Python wrapper example

See `integration/algotradeplan/hub_bridge.py` for a reference subprocess wrapper.
Copy the relevant parts or adapt the pattern to your project:

```python
import json, os, subprocess

def run_bridge(command, *, stdin=""):
    binary = os.environ["MARKET_DATA_BIN"]
    result = subprocess.run(
        [binary, *command],
        input=stdin, text=True, capture_output=True, check=True,
    )
    return json.loads(result.stdout)

sources = run_bridge(["sources"])
caps    = run_bridge(["capabilities"])

raw_payload = json.dumps({"kline": [[1716200000000, 100, 110, 90, 105, 1000]]})
result = run_bridge(
    ["ingest", "--source", "binance_futures", "--symbol", "BTCUSDT",
     "--datasets", "kline", "--asset-type", "crypto_spot"],
    stdin=raw_payload,
)
print(result["quality_report"]["passed"])
```

## Option 3 - gRPC microservice (planned - Phase 3)

A future release will expose the same contract over gRPC (`tonic`), enabling
remote consumption over the network without a local binary. Pin to
`contract_version: 1` now and the gRPC service will keep the same contract fields.

Use `sources`, `capabilities`, and `query-dataset-summary` at runtime to discover
the current supported source + dataset matrix instead of hard-coding static lists.

---

## Staying compatible across releases

1. Always call `doctor` first and assert the `contract_version` you built against.
2. Do not depend on internal struct field ordering of JSON output; use field names.
3. New dataset types and new sources are additive and non-breaking.
4. Breaking changes increment `contract_version`.
