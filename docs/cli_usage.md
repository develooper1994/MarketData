# MarketData Bridge CLI Usage

`market_data_bridge` is the operational CLI for discovery, recommendation, and
ingestion.

## Help menu

```bash
market_data_bridge help
market_data_bridge --help
```

## Command cheatsheet

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
| `supported-use-cases` | Built-in recommendation flows |
| `recommend-sources --use-case <name> [--limit N]` | Use-case recommendation list |
| `ingest --source <name> --symbol <id> --datasets <csv>` | Normalize + quality + storage + provenance |

## Common flows

### 1) Verify runtime + contract

```bash
market_data_bridge doctor
market_data_bridge assert-contract --expected 1
```

### 2) Discover source coverage

```bash
market_data_bridge sources
market_data_bridge capabilities
market_data_bridge query-sources-for --dataset kline --asset-class crypto_spot
```

### 3) Select recommended sources

```bash
market_data_bridge query-best-sources --dataset kline --asset-class crypto_spot --limit 5
market_data_bridge query-source-summary --source binance_futures
market_data_bridge query-dataset-summary --dataset kline
market_data_bridge supported-use-cases
market_data_bridge recommend-sources --use-case crypto_backtest --limit 5
```

### 4) Run full ingest pipeline

```bash
printf '{"kline":[[1716200000000,100,110,90,105,1000]]}' | \
  market_data_bridge ingest \
    --source offline \
    --symbol BTCUSDT \
    --datasets kline \
    --asset-type crypto_spot
```
