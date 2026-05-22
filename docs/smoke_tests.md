# Smoke tests — Manual live-fetch instructions

This document explains how to run one-off live smoke-tests for providers. The repository contains a small helper script and a sample symbol mapping to make manual testing reproducible.

Prerequisites
- Rust toolchain (`cargo`) installed
- Network access from the host running the commands
- Optional: `TEFAS_CLI_CMD` set to a built `tefas-cli` binary for TEFAS provider

Useful files
- `configs/sample_symbols.json`: known-good symbols per provider for smoke-tests
- `scripts/setup_external_tools.sh`: clones and builds `tefas-cli` into `external_tools/`
- `scripts/run_marketdata_ingest.sh`: convenience script that runs a set of provider ingest commands and saves outputs to `ingest_outputs/`

Running a single-provider live fetch

1) Use the sample symbol (example for `btcturk`):

```bash
cargo run --bin market_data_bridge -- live-fetch --source btcturk --symbol BTCUSDT --dataset tick
```

2) TEFAS (preferred CLI-first flow):

Build `tefas-cli` and export `TEFAS_CLI_CMD`:

```bash
./scripts/setup_external_tools.sh
export TEFAS_CLI_CMD=$(pwd)/external_tools/tefas-cli/target/release/cli
cargo run --bin market_data_bridge -- live-fetch --source tefas --symbol 0001 --dataset fundamentals
```

3) Scraping providers

Scraping-based sources are opt-in to avoid accidental scraping. Enable them with `ENABLE_SCRAPING_PROVIDERS=true`.

```bash
ENABLE_SCRAPING_PROVIDERS=true cargo run --bin market_data_bridge -- live-fetch --source fintables --symbol GARAN --dataset fundamentals
```

Diagnosing common failures
- `http_403` or Cloudflare blocks: try a `curl -I` to inspect headers and check whether the host requires additional headers or a different endpoint. Some targets may actively block automated clients.
- `http_404`: confirm the sample symbol and endpoint path. Try variations (e.g. `USDTRY` vs `USD`) or check provider docs.
- `Sistem Hatası!!` from TEFAS: the CLI returned structured JSON but the service reported an internal error for that symbol; try alternate symbol codes.

Saving outputs
- The convenience script `scripts/run_marketdata_ingest.sh` runs a suite of providers and saves outputs to `ingest_outputs/`.

Notes & best practices
- Run providers one at a time as the CLI does; do not script many parallel fetches without rate-limit/backoff handling.
- Use `configs/sample_symbols.json` as a starting point; add more symbols you validate manually.
- If a provider requires API keys or has strict TOS, add credentials to a local-only environment file and do not commit them to the repository.
