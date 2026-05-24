Upgrade & Migration Notes — MarketData

Overview

This doc summarizes recent automatic improvements applied to the `market_data` crate and how to adopt them.

What I changed

- Moved large TEFAS example datasets out of `external_tools/tefas-cli/datasets` into `artifacts/tefas-datasets/datasets/` and added `.gitignore` entries so those files are no longer tracked.
- Added an optional async provider prototype: `src/providers/yahoo_async.rs` (feature `async_providers`).
- Added a parallel raw-fetch path for `Etl::fetch` (feature `parallel`). This parallelizes the provider network fetch phase using `rayon` and then performs normalization and storage sequentially.
- Replaced regex-based interleaved matching with a fast subsequence check in `src/matcher.rs` to speed up matching.
- Made `LocalArtifactStorage::write` atomic by writing to a temp file and renaming (safer on crashes).
- Added an S3 storage backend scaffold behind feature `s3` (placeholder implementation).
- Bench scaffold added: `benches/etl_bench.rs` (uses `criterion`).
- CI improvements and tooling were already applied (clippy -D warnings, cargo-audit, tarpaulin coverage).

How to enable new features

- Parallel raw fetch (rayon): enable the `parallel` feature when building or running:

```bash
# run tests with parallel fetch enabled
cargo test -p market_data --features parallel

# run bench with parallel enabled
cargo bench -p market_data --features parallel
```

- Async provider prototypes: enable `async_providers` feature to compile the async adapter prototype.

```bash
cargo build -p market_data --features async_providers
```

- Metrics: enable `metrics` feature to compile instrumentation points (you still need to provide a metrics exporter at runtime).

```bash
cargo build -p market_data --features metrics
```

Notes & Recommendations

- The parallel fetch path only parallelizes the adapter/raw-fetch phase. Normalization and storage still happen sequentially to preserve the existing provenance/storage model and avoid complex locking.
- For a full async pipeline: migrate `DataHub` and storage/provenance components to async-friendly implementations (feature `async_providers` was added as a first step).
- Consider hosting the TEFAS sample data in a release asset or object store (S3 / Git LFS) instead of keeping them in the repo history.

If you want, I can:
- Open a draft PR with these changes grouped by concern, or
- Split the work into smaller PRs (CI + docs; storage + S3 scaffold; parallel fetch + bench; async providers scaffold).
