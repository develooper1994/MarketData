"""Drop-in replacement for AlgoTradePlan's ``data/hub.py``.

This module provides exactly the same public API as the original ``DataHub``
but with the data-processing layer (normalize / quality / storage / provenance)
removed from Python and delegated entirely to the ``market_data_bridge`` Rust
binary from ``develooper1994/MarketData``.

Migration notes
---------------
1. Replace ``from src.algotradeplan.data.hub import DataHub`` with
   ``from integration.algotradeplan.hub_bridge import DataHub`` (or copy this
   file to ``src/algotradeplan/data/hub.py``).
2. Build the Rust binary first::

       cargo build --release --bin market_data_bridge

3. Set ``MARKET_DATA_BIN`` to point at the compiled binary, or set
   ``MARKET_DATA_REPO`` to the ``MarketData`` repo root so the bridge falls
   back to ``cargo run``.
4. Remove ``src/algotradeplan/data/`` modules after smoke tests pass. Keep
   only ``__init__.py`` and this bridge-backed ``hub.py`` compatibility shim.
"""

from __future__ import annotations

import json
import os
import shlex
import subprocess
from dataclasses import asdict, dataclass, field, is_dataclass
from pathlib import Path
from typing import Any, Callable

# ---------------------------------------------------------------------------
# Contracts – re-exported so callers do not need to change their imports.
# ---------------------------------------------------------------------------
from src.algotradeplan.plugins.data.contracts import (  # type: ignore[import]
    DataRecord,
    DataRequest,
    ProvenanceRecord,
    QualityReport,
    StorageReceipt,
)

JsonGetter = Callable[[str, dict[str, Any]], Any]
RawDatasetFetcher = Callable[[str, str, list[str], str, int, dict[str, Any]], dict[str, Any]]
AssetDiscoverer = Callable[[str, int, dict[str, Any]], list[str]]


# ---------------------------------------------------------------------------
# IngestResult – identical to the original hub.py definition.
# ---------------------------------------------------------------------------

@dataclass(frozen=True)
class IngestResult:
    source: str
    symbol: str | None
    requested_datasets: list[str]
    dataset_coverage: dict[str, int]
    raw_datasets: dict[str, Any]
    normalized: dict[str, list[dict[str, Any]]]
    records: list[DataRecord]
    quality_report: QualityReport
    storage_receipts: list[StorageReceipt] = field(default_factory=list)
    provenance: ProvenanceRecord | None = None
    source_issues: list[dict[str, str]] = field(default_factory=list)

    def to_feature_frame(self, dataset: str | None = None):
        rows = []
        for record in self.records:
            if dataset and record.metadata.get("dataset") != dataset:
                continue
            rows.append(record.payload)
        try:
            import pandas as pd  # type: ignore
        except Exception:  # pragma: no cover
            return rows
        return pd.DataFrame(rows)


# ---------------------------------------------------------------------------
# Internal bridge helper
# ---------------------------------------------------------------------------

_ALLOWED_API_PREFIXES = (
    "https://fapi.binance.com/",
    "https://api.bybit.com/",
    "https://api.kraken.com/",
    "https://api.exchange.coinbase.com/",
    "https://query1.finance.yahoo.com/",
    "https://www.alphavantage.co/",
    "https://api.twelvedata.com/",
    "https://api.polygon.io/",
    "https://finnhub.io/",
    "https://data.nasdaq.com/",
    "https://cloud.iexapis.com/",
    "https://api.frankfurter.dev/",
    "https://hn.algolia.com/",
    "https://api.coingecko.com/",
    "https://stooq.com/",
    "https://api.gdeltproject.org/",
    "https://api.worldbank.org/",
    "https://data-api.ecb.europa.eu/",
    "https://api.llama.fi/",
    "https://api.stlouisfed.org/",
    "https://data.sec.gov/",
    "https://www.sec.gov/",
    "https://financialmodelingprep.com/",
)


def _run_bridge(
    command: list[str],
    *,
    input: str = "",
    binary: str | None = None,
    repo_root: Path | None = None,
) -> Any:
    """Run a ``market_data_bridge`` subcommand and return parsed JSON."""
    binary = binary or os.getenv("MARKET_DATA_BIN")
    # Allow injecting extra args for the bridge binary via env var.
    # Example: MARKET_DATA_BIN_ARGS="--timeout 30 --verbose"
    extra_args_raw = os.getenv("MARKET_DATA_BIN_ARGS", "")
    extra_args: list[str] = shlex.split(extra_args_raw) if extra_args_raw else []
    if binary:
        full_command = [binary, *extra_args, *command]
        cwd = None
    else:
        root = repo_root or Path(os.getenv("MARKET_DATA_REPO", ".")).resolve()
        full_command = [
            "cargo",
            "run",
            "--quiet",
            "--bin",
            "market_data_bridge",
            "--",
            *extra_args,
            *command,
        ]
        cwd = str(root)

    result = subprocess.run(
        full_command,
        cwd=cwd,
        input=input,
        text=True,
        capture_output=True,
        check=True,
    )
    return json.loads(result.stdout)


# ---------------------------------------------------------------------------
# Capability metadata – loaded from Rust on first use.
# ---------------------------------------------------------------------------

class _CapabilityProxy:
    """Lazy capability registry backed by the Rust binary."""

    def __init__(self, binary: str | None = None, repo_root: Path | None = None) -> None:
        self._binary = binary
        self._repo_root = repo_root
        self._cache: list[dict[str, Any]] | None = None
        self._map_cache: dict[str, dict[str, Any]] | None = None

    def _all(self) -> list[dict[str, Any]]:
        if self._cache is None:
            self._cache = _run_bridge(
                ["capabilities"],
                binary=self._binary,
                repo_root=self._repo_root,
            )  # type: ignore[assignment]
        return self._cache  # type: ignore[return-value]

    def map(self) -> dict[str, dict[str, Any]]:
        if self._map_cache is None:
            self._map_cache = {cap["source"]: cap for cap in self._all()}
        return self._map_cache

    def sources(self) -> list[str]:
        return sorted(self.map())

    def get(self, source: str) -> dict[str, Any] | None:
        return self.map().get(source)


# ---------------------------------------------------------------------------
# DataHub
# ---------------------------------------------------------------------------

class DataHub:
    """
    Public API-compatible replacement for the original AlgoTradePlan DataHub.

    The internal data-processing pipeline (normalize / quality / storage /
    provenance) is now fully owned by the ``market_data_bridge`` binary from
    ``develooper1994/MarketData``.  Python retains ownership of:
    - raw HTTP source adapters (``_adapter_registry``)
    - capability metadata queries (delegated to the Rust binary)
    - ETL/orchestration façade
    """

    def __init__(
        self,
        *,
        json_getter: JsonGetter | None = None,
        storage: Any | None = None,          # kept for API compat, ignored
        provenance: Any | None = None,        # kept for API compat, ignored
        artifact_root: Path | None = None,    # kept for API compat, passed to bridge
        raw_fetcher: RawDatasetFetcher | None = None,
        asset_discoverer: AssetDiscoverer | None = None,
        binary: str | None = None,
        repo_root: Path | os.PathLike[str] | None = None,
    ) -> None:
        # Resolve bridge binary / repo root for all subprocess calls.
        self._binary = binary or os.getenv("MARKET_DATA_BIN")
        self._repo_root = Path(repo_root or os.getenv("MARKET_DATA_REPO", ".")).resolve()
        self._artifact_root = artifact_root or Path("artifacts") / "datahub"

        self._caps = _CapabilityProxy(
            binary=self._binary, repo_root=self._repo_root
        )
        self._raw_fetcher = raw_fetcher
        self._asset_discoverer = asset_discoverer

    # ------------------------------------------------------------------
    # Capability / query API – identical signatures to original hub.py
    # ------------------------------------------------------------------

    def sources(self) -> list[str]:
        return self._caps.sources()

    def capability(self, source: str) -> dict[str, Any]:
        cap = self._caps.get(source)
        if cap is None:
            raise KeyError(source)
        return cap

    def coverage_table(self) -> list[dict[str, str]]:
        rows: list[dict[str, str]] = []
        for cap in self._caps._all():
            source = str(cap.get("source", ""))
            for dataset in cap.get("datasets", []):
                rows.append(
                    {
                        "source": source,
                        "dataset": str(dataset),
                        "status": _query_dataset_status(cap, str(dataset)),
                    }
                )
        return rows

    def dataset_status(self, source: str, dataset: str) -> str:
        return _query_dataset_status(self._caps.get(source), dataset)

    def asset_status(self, source: str, asset_class: str) -> str:
        cap = self._caps.get(source)
        if cap is None:
            return "unsupported"
        if asset_class in cap.get("asset_classes", []):
            return cap.get("implementation_status", "unsupported")
        return "unsupported"

    def supports(self, source: str, dataset: str, *, require_live: bool = False) -> bool:
        cap = self._caps.get(source)
        if cap is None:
            return False
        if require_live and not cap.get("supports_realtime", False):
            return False
        canonical = _canonical(dataset)
        return canonical in cap.get("implemented_datasets", [])

    def sources_for(
        self,
        dataset: str | None = None,
        asset_class: str | None = None,
        require_live: bool = False,
    ) -> list[str]:
        args = ["query-sources-for"]
        if dataset:
            args += ["--dataset", dataset]
        if asset_class:
            args += ["--asset-class", asset_class]
        if require_live:
            args.append("--require-live")
        return _run_bridge(args, binary=self._binary, repo_root=self._repo_root)  # type: ignore[return-value]

    def available_datasets(self, source: str, *, implemented_only: bool = False) -> list[str]:
        cap = self._caps.get(source)
        if cap is None:
            return []
        key = "implemented_datasets" if implemented_only else "datasets"
        return cap.get(key, [])

    def requires_api_key(self, source: str) -> bool:
        cap = self._caps.get(source)
        return bool(cap and cap.get("requires_api_key", False))

    def api_key_env(self, source: str) -> str | None:
        cap = self._caps.get(source)
        return cap.get("api_key_env") if cap else None

    def compare_sources(self, sources: list[str], datasets: list[str] | None = None) -> list[dict[str, str]]:
        caps = self._caps.map()
        rows: list[dict[str, str]] = []
        for source in sources:
            cap = caps.get(source)
            if cap is None:
                continue
            candidate_datasets = datasets or cap.get("datasets", [])
            implemented = {
                _canonical(ds) for ds in cap.get("implemented_datasets", [])
            }
            rows.append(
                {
                    "source": source,
                    "quality_level": str(cap.get("quality_level", "")),
                    "implementation_status": str(cap.get("implementation_status", "")),
                    "supports_realtime": str(bool(cap.get("supports_realtime", False))).lower(),
                    "requires_api_key": str(bool(cap.get("requires_api_key", False))).lower(),
                    "implemented_dataset_count": str(
                        sum(
                            1
                            for dataset in candidate_datasets
                            if _canonical(str(dataset)) in implemented
                        )
                    ),
                }
            )
        return rows

    def source_summary(self, source: str) -> dict[str, Any]:
        return self._caps.get(source) or {}

    def best_sources_for(
        self,
        *,
        dataset: str,
        asset_class: str | None = None,
        prefer_live: bool = True,
        allow_api_key: bool = True,
        include_metadata_only: bool = False,
        limit: int | None = None,
    ) -> list[dict[str, str]]:
        args = ["query-best-sources", "--dataset", _canonical(dataset)]
        if asset_class:
            args += ["--asset-class", asset_class]
        if not allow_api_key:
            args.append("--disallow-api-key")
        if not prefer_live:
            args.append("--no-prefer-live")
        if include_metadata_only:
            args.append("--include-metadata-only")
        if limit is not None:
            args += ["--limit", str(max(0, limit))]
        return _run_bridge(args, binary=self._binary, repo_root=self._repo_root)  # type: ignore[return-value]

    def explain_source(self, source: str) -> dict[str, Any]:
        return _run_bridge(
            ["query-source-summary", "--source", source],
            binary=self._binary,
            repo_root=self._repo_root,
        )

    def explain_dataset(self, dataset: str) -> dict[str, Any]:
        return _run_bridge(
            ["query-dataset-summary", "--dataset", _canonical(dataset)],
            binary=self._binary,
            repo_root=self._repo_root,
        )

    def recommend_sources(
        self,
        use_case: str,
        *,
        allow_api_key: bool = True,
        prefer_live: bool = True,
        limit: int | None = None,
    ) -> list[dict[str, str]]:
        args = ["recommend-sources", "--use-case", use_case]
        if not allow_api_key:
            args.append("--disallow-api-key")
        if not prefer_live:
            args.append("--no-prefer-live")
        if limit is not None:
            args += ["--limit", str(max(0, limit))]
        return _run_bridge(
            args,
            binary=self._binary,
            repo_root=self._repo_root,
        )

    def supported_use_cases(self) -> list[str]:
        return _run_bridge(
            ["supported-use-cases"],
            binary=self._binary,
            repo_root=self._repo_root,
        )  # type: ignore[return-value]

    def dataset_sources_matrix(self, datasets: list[str] | None = None) -> list[dict[str, str]]:
        all_datasets = sorted(
            {_canonical(ds) for cap in self._caps._all() for ds in cap.get("datasets", [])}
        )
        selected = [_canonical(ds) for ds in (datasets or all_datasets)]
        rows: list[dict[str, str]] = []
        for dataset in selected:
            rows.append(
                {
                    "dataset": dataset,
                    "sources": ",".join(self.sources_for(dataset=dataset)),
                    "live_sources": ",".join(
                        self.sources_for(dataset=dataset, require_live=True)
                    ),
                }
            )
        return rows

    def asset_sources_matrix(self, asset_classes: list[str] | None = None) -> list[dict[str, str]]:
        all_asset_classes = sorted(
            {asset for cap in self._caps._all() for asset in cap.get("asset_classes", [])}
        )
        selected = asset_classes or all_asset_classes
        rows: list[dict[str, str]] = []
        for asset_class in selected:
            rows.append(
                {
                    "asset_class": asset_class,
                    "sources": ",".join(self.sources_for(asset_class=asset_class)),
                    "live_sources": ",".join(
                        self.sources_for(asset_class=asset_class, require_live=True)
                    ),
                }
            )
        return rows

    def discover_assets(self, source: str, limit: int = 10, **filters: Any) -> list[str]:
        cap = self._caps.get(source)
        if not cap or not cap.get("supports_discovery", False):
            return []
        if self._asset_discoverer is None:
            return []
        symbols = self._asset_discoverer(source, max(1, limit), filters)
        return symbols[:limit]

    # ------------------------------------------------------------------
    # Ingest – delegates normalize / quality / storage / provenance to Rust
    # ------------------------------------------------------------------

    def ingest(
        self,
        *,
        source: str,
        symbol: str,
        datasets: list[str],
        timeframe: str = "1m",
        limit: int = 500,
        allow_partial: bool = False,
        store: bool = True,
        **fetch_options: Any,
    ) -> IngestResult:
        cap = self._caps.get(source)
        requested = [_canonical(ds) for ds in datasets]
        source_issues_by_dataset: dict[str, str] = {}
        fetchable: list[str] = []

        if cap is None:
            return _empty_ingest_result(source, symbol, requested, "unknown_source")

        api_key_required_env = cap.get("api_key_env") if cap.get("requires_api_key") else None
        api_key_missing = bool(api_key_required_env and not os.getenv(api_key_required_env))

        for dataset in requested:
            status = _query_dataset_status(cap, dataset)
            if status == "unsupported":
                source_issues_by_dataset[dataset] = f"unsupported_dataset:{dataset}"
                continue
            if status == "metadata_only":
                source_issues_by_dataset[dataset] = f"metadata_only_dataset:{dataset}"
                continue
            if status in {"api_key", "api_key_or_plan"} and api_key_missing and api_key_required_env:
                source_issues_by_dataset[dataset] = f"api_key_required:{api_key_required_env}"
                continue
            fetchable.append(dataset)

        raw_datasets = fetch_options.get("raw_datasets", {})
        if not isinstance(raw_datasets, dict):
            raw_datasets = {}
        raw_datasets_provided = bool(raw_datasets)
        fetch_attempted = False
        if fetchable and self._raw_fetcher is not None and not raw_datasets:
            # `raw_datasets` is bridge-owned input; do not forward it to callback options.
            fetch_options_for_raw = dict(fetch_options)
            fetch_options_for_raw.pop("raw_datasets", None)
            fetch_attempted = True
            raw_datasets = self._raw_fetcher(
                source,
                symbol,
                fetchable,
                timeframe,
                limit,
                fetch_options_for_raw,
            )
            if not isinstance(raw_datasets, dict):
                raw_datasets = {}
        # Only report that Python-side raw datasets are required when a
        # Python raw_fetcher is present (i.e. we won't delegate fetching to
        # the Rust bridge). If no Python fetcher exists, we'll let the
        # bridge perform the fetch instead.
        should_report_raw_dataset_required = (
            fetchable
            and not raw_datasets
            and not raw_datasets_provided
            and not fetch_attempted
            and self._raw_fetcher is not None
        )
        if should_report_raw_dataset_required:
            for dataset in fetchable:
                source_issues_by_dataset[dataset] = "raw_dataset_required"
        else:
            # If there is no Python raw_fetcher, we will delegate fetching to
            # the Rust bridge; do not mark datasets as "missing_raw_dataset"
            # in that case since the bridge will attempt to fetch them.
            if not (fetchable and self._raw_fetcher is None):
                for dataset in fetchable:
                    if dataset not in raw_datasets:
                        source_issues_by_dataset[dataset] = "missing_raw_dataset"

        # Record all non-fetchable datasets as source issues.
        issues: list[dict[str, str]] = [
            {"source": source, "reason": reason}
            for ds, reason in source_issues_by_dataset.items()
        ]

        if not raw_datasets:
            # If Python-side raw fetchers are not provided, delegate fetching to
            # the Rust bridge in JSON mode so CLI and bridge-backed calls behave
            # the same. This allows `DataHub.ingest(...)` to be used without
            # a Python adapter while keeping parity with `market_data_bridge`.
            if fetchable and self._raw_fetcher is None:
                bridge_payload: dict[str, Any] = {
                    "source": source,
                    "symbol": symbol,
                    "datasets": fetchable,
                    "store": store,
                }
                # Propagate optional roots and duckdb path if provided.
                bridge_payload["record_root"] = fetch_options.get(
                    "record_root", str(self._artifact_root / "records")
                )
                bridge_payload["manifest_root"] = fetch_options.get(
                    "manifest_root", str(self._artifact_root / "manifests")
                )
                if "duckdb" in fetch_options and fetch_options.get("duckdb"):
                    bridge_payload["duckdb"] = fetch_options.get("duckdb")

                response = _run_bridge(
                    ["ingest", "--json"],
                    input=json.dumps(bridge_payload),
                    binary=self._binary,
                    repo_root=self._repo_root,
                )

                return _parse_ingest_result(response, issues)

            # Nothing to send to Rust; short-circuit when no fetcher available.
            return IngestResult(
                source=source,
                symbol=symbol,
                requested_datasets=requested,
                dataset_coverage={ds: 0 for ds in requested},
                raw_datasets={},
                normalized={},
                records=[],
                quality_report=QualityReport(
                    passed=False,
                    checks=[],
                    issues=[i["reason"] for i in issues],
                ),
                source_issues=issues,
            )

        # Hand payload to Rust for normalize + quality + storage + provenance.
        asset_type = cap.get("asset_classes", ["unknown"])[0]
        command = [
            "ingest",
            "--source", source,
            "--symbol", symbol,
            "--datasets", ",".join(raw_datasets.keys()),
            "--asset-type", asset_type,
        ]
        if store:
            command.append("--store")
            record_root = fetch_options.get(
                "record_root",
                str(self._artifact_root / "records"),
            )
            manifest_root = fetch_options.get(
                "manifest_root",
                str(self._artifact_root / "manifests"),
            )
            command += ["--record-root", str(record_root)]
            command += ["--manifest-root", str(manifest_root)]
        # Forward optional duckdb path to the bridge so it can attempt import.
        if "duckdb" in fetch_options and fetch_options.get("duckdb"):
            command += ["--duckdb", str(fetch_options.get("duckdb"))]

        response = _run_bridge(
            command,
            input=json.dumps(raw_datasets),
            binary=self._binary,
            repo_root=self._repo_root,
        )

        return _parse_ingest_result(response, issues)

    def load_market_data(
        self,
        *,
        source: str,
        symbol: str,
        dataset: str,
        timeframe: str = "1m",
        limit: int = 500,
        allow_partial: bool = False,
        **fetch_options: Any,
    ):
        return self.ingest(
            source=source,
            symbol=symbol,
            datasets=[dataset],
            timeframe=timeframe,
            limit=limit,
            allow_partial=allow_partial,
            store=False,
            **fetch_options,
        ).to_feature_frame(dataset=_canonical(dataset))


# ---------------------------------------------------------------------------
# Private helpers
# ---------------------------------------------------------------------------

def _canonical(dataset: str) -> str:
    _ALIASES = {
        "ohlcv": "kline",
        "ticker": "tick",
        "trades": "trade",
        "book": "orderbook",
        "macro_snapshot": "macro",
        "macro_series": "macro",
    }
    return _ALIASES.get(dataset.lower(), dataset.lower())


def _query_dataset_status(cap: dict[str, Any] | None, dataset: str) -> str:
    if cap is None:
        return "unsupported"
    canonical = _canonical(dataset)
    if canonical not in cap.get("datasets", []):
        return "unsupported"
    if canonical in cap.get("metadata_only_datasets", []):
        return "metadata_only"
    if canonical not in cap.get("implemented_datasets", []):
        return "metadata_only"
    return cap.get("implementation_status", "unsupported")


def _empty_ingest_result(
    source: str, symbol: str, requested: list[str], reason: str
) -> IngestResult:
    return IngestResult(
        source=source,
        symbol=symbol,
        requested_datasets=requested,
        dataset_coverage={ds: 0 for ds in requested},
        raw_datasets={},
        normalized={},
        records=[],
        quality_report=QualityReport(passed=False, checks=[], issues=[reason]),
        source_issues=[{"source": source, "reason": reason}],
    )


def _parse_ingest_result(
    response: dict[str, Any],
    extra_issues: list[dict[str, str]],
) -> IngestResult:
    quality_report = QualityReport(**response["quality_report"])
    storage_receipts = [StorageReceipt(**item) for item in response.get("storage_receipts", [])]
    provenance = None
    if prov_payload := response.get("provenance"):
        provenance = ProvenanceRecord(
            request=DataRequest(**prov_payload["request"]),
            source_plugin_id=prov_payload["source_plugin_id"],
            storage_receipts=[StorageReceipt(**item) for item in prov_payload["storage_receipts"]],
            record_keys=prov_payload["record_keys"],
            revision=prov_payload["revision"],
        )
    records = [DataRecord(**item) for item in response.get("records", [])]
    all_issues = extra_issues + response.get("source_issues", [])
    return IngestResult(
        source=response["source"],
        symbol=response.get("symbol"),
        requested_datasets=response.get("requested_datasets", []),
        dataset_coverage=response.get("dataset_coverage", {}),
        raw_datasets=response.get("raw_datasets", {}),
        normalized=response.get("normalized", {}),
        records=records,
        quality_report=quality_report,
        storage_receipts=storage_receipts,
        provenance=provenance,
        source_issues=all_issues,
    )
