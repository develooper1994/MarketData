PR Summary

- Add asset-class selection docs (`MarketData/docs/selection_explain.md`) and updated CLI help (`MarketData/src/bin/market_data_bridge.rs`).
- Add GitHub Actions CI workflow (`.github/workflows/ci.yml`) running tests across Rust channels and providing a manual gate for live/network tests.
- Format code with `cargo fmt` and run `cargo clippy` for inspection.
- Run `cargo test` for the `market_data` package and commit changes.

Testing

- Local run: `cargo test --manifest-path MarketData/Cargo.toml --package market_data -- --nocapture` passed.

Notes

- Use the `workflow_dispatch` input `run_live_tests=true` to trigger the `live-tests` job which sets `MARKET_DATA_LIVE_TESTS=1` for network tests.
- CI currently runs `cargo clippy` but does not fail on warnings; consider tightening to `-D warnings` after addressing existing warnings.

Reviewer checklist

- [ ] Verify CI config and runner selection.
- [ ] Confirm `--asset-class` / `--force-asset-class` behavior in runtime scenarios.
- [ ] Decide whether to enforce clippy warnings as errors in CI.
