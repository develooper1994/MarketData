# Asset-class selection and matching semantics

This document explains how asset-class hints and the `--force-asset-class` option affect source selection and ingestion in MarketData.

## Overview

- `--asset-class <name>` (hint): a preference passed through the CLI into `IngestOptions` and propagated to the selection and ingestion stack as `requested_asset_class`. It biases source selection and adapter behavior but does not prevent fallbacks.
- `--force-asset-class` (force): when set the system will only accept sources that explicitly advertise support for the requested asset class (according to the internal capability map). If none are available the hub returns an empty result with a `source_issue` containing `unsupported_asset_class:<name>`.

Both `requested_asset_class` and `force_asset_class` are propagated end-to-end: CLI → `IngestOptions` → `SourceSelector` / registry → `RawSourceAdapter::fetch_raw` → `DataHub::ingest_from_raw` → provenance (`DataRequest.parameters`).

## Hint semantics (preference)

- When you pass `--asset-class crypto_spot` without `--force-asset-class` the selection logic prefers sources that list `crypto_spot` in their capability metadata.
- If preferred sources cannot satisfy the dataset or fail, the selector will fallback to other sources (different asset classes) to maximize coverage.
- Adapters may use the `requested_asset_class` parameter as a hint to prefer APIs or endpoints that better match the asset class (for example, choosing a coin ID endpoint vs an FX endpoint), but adapters are not required to enforce the hint.

## Force semantics (strict enforcement)

- When `--force-asset-class` is supplied the hub verifies that the chosen source advertises the `requested_asset_class` in the capability map. If it does not, the hub short-circuits and returns a result with `dataset_coverage` set to zero and a `source_issue` note of the form `unsupported_asset_class:<name>`.
- Provider adapters are also wrapped with a thin `AdapterWrapper` that performs the same check; adapters will return an empty payload when called with a forced but unsupported asset class (defense-in-depth).
- This mode is useful when the caller must avoid mixed-class fallbacks (e.g., treat `crypto_spot` pricing differently from FX references).

## Matching precedence (symbol matching examples)

When selecting candidate assets and matching symbol-like queries, MarketData's matcher ranks candidates by specificity. Example for a query `EUR` (the matching precedence is illustrative):

1) Exact / prefix matches
   - `EUR` (exact)
   - `EUR*` → `EURQWE`, `EURASD`, `EURZXC`, ...
2) Suffix matches
   - `*EUR` → `ASDEUR`, `ZXCEUR`, `QWEEUR`, ...
3) Contained matches
   - `*EUR*` → `ZXCEUR`, `QWEEUR`, `ASDEUR`, ...
4) Interleaved/wildcard matches
   - `*E*U*R*` → any symbol containing E, U, R in order (but not contiguous)

The matcher prefers earlier groups (exact/prefix) over later groups (suffix/contained) to reduce false positives and surface the most likely canonical candidates first.

## Provenance and observability

- When ingesting with `--asset-class`/`--force-asset-class`, the `DataRequest.parameters` recorded in the provenance manifest include `requested_asset_class` (string) and `force_asset_class` (bool) so downstream consumers can audit how data was selected.

## CLI examples

- Prefer crypto spot sources:

  market_data_bridge query-sources-for --dataset kline --asset-class crypto_spot

- Require crypto spot sources (fail if none):

  market_data_bridge query-best-sources --dataset kline --asset-class crypto_spot --limit 3 --force-asset-class

- Ingest with hint (asset_type is separate from asset_class):

  printf '{"kline":[[1716200000000,"10","11","9","10.5","42"]]}' | \
    market_data_bridge ingest --source auto --symbol BTCUSDT --datasets kline --asset-class crypto_spot --asset-type crypto_spot

## Notes and best practices

- Use `--asset-class` when you have a strong expectation about the product class of the symbol (e.g., `mutual_fund` for TEFAS funds, `macro` for macro series).
- Use `--force-asset-class` only when fallbacks would be harmful (for example mixing NAVs with market prices).
- The selection help in the CLI points to this document; if you need further customization per-adapter, adapt the adapter implementation to honour `requested_asset_class` more deeply.
