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
4. Remove ``src/algotradeplan/data/normalize.py``,
   ``src/algotradeplan/data/quality.py``,
   ``src/algotradeplan/data/storage.py``, and
   ``src/algotradeplan/data/provenance.py`` once the smoke tests below pass.

Files that should remain in AlgoTradePlan after the cutover
-----------------------------------------------------------
- ``src/algotradeplan/data/adapters/`` – raw HTTP source connectors
- ``src/algotradeplan/data/etl.py``    – ETL facade (uses this hub)
- ``src/algotradeplan/data/coverage.py`` – convenience table builder
- ``src/algotradeplan/data/query.py``    – capability query helpers
- ``src/algotradeplan/plugins/``         – plugin interfaces and examples
"""

from __future__ import annotations

import json
import os
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
) -> dict[str, Any]:
    """Run a ``market_data_bridge`` subcommand and return parsed JSON."""
    binary = binary or os.getenv("MARKET_DATA_BIN")
    if binary:
        full_command = [binary, *command]
        cwd = None
    else:
        root = repo_root or Path(os.getenv("MARKET_DATA_REPO", ".")).resolve()
        full_command = ["cargo", "run", "--quiet", "--bin", "market_data_bridge", "--", *command]
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

        # Import adapter registry lazily; keeps this file usable standalone.
        try:
            from src.algotradeplan.data.adapters import (  # type: ignore[import]
                build_default_adapter_registry,
            )
            # json_getter falls back to the bridge-internal getter when None.
            from src.algotradeplan.data.hub import _default_json_getter  # type: ignore[import]
            getter = json_getter or _default_json_getter
            self._adapter_registry = build_default_adapter_registry(getter)
        except ImportError:
            self._adapter_registry = None  # type: ignore[assignment]

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
        from src.algotradeplan.data.coverage import build_coverage_table  # type: ignore[import]
        return build_coverage_table(self._caps._all())

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
        from src.algotradeplan.data.query import compare_sources  # type: ignore[import]
        return compare_sources(self._caps.map(), sources, datasets)

    def source_summary(self, source: str) -> dict[str, Any]:
        from src.algotradeplan.data.query import source_summary  # type: ignore[import]
        return source_summary(self._caps.map(), source)

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
        from src.algotradeplan.data.query import best_sources_for  # type: ignore[import]
        return best_sources_for(
            self._caps.map(),
            dataset=dataset,
            asset_class=asset_class,
            prefer_live=prefer_live,
            allow_api_key=allow_api_key,
            include_metadata_only=include_metadata_only,
            limit=limit,
        )

    def explain_source(self, source: str) -> dict[str, Any]:
        from src.algotradeplan.data.query import explain_source  # type: ignore[import]
        return explain_source(self._caps.map(), source)

    def explain_dataset(self, dataset: str) -> dict[str, Any]:
        from src.algotradeplan.data.query import explain_dataset  # type: ignore[import]
        return explain_dataset(self._caps.map(), dataset)

    def recommend_sources(
        self,
        use_case: str,
        *,
        allow_api_key: bool = True,
        prefer_live: bool = True,
        limit: int | None = None,
    ) -> list[dict[str, str]]:
        from src.algotradeplan.data.query import recommend_sources  # type: ignore[import]
        return recommend_sources(
            self._caps.map(),
            use_case,
            allow_api_key=allow_api_key,
            prefer_live=prefer_live,
            limit=limit,
        )

    def supported_use_cases(self) -> list[str]:
        from src.algotradeplan.data.query import supported_use_cases  # type: ignore[import]
        return supported_use_cases()

    def dataset_sources_matrix(self, datasets: list[str] | None = None) -> list[dict[str, str]]:
        from src.algotradeplan.data.query import dataset_sources_matrix  # type: ignore[import]
        return dataset_sources_matrix(self._caps.map(), datasets)

    def asset_sources_matrix(self, asset_classes: list[str] | None = None) -> list[dict[str, str]]:
        from src.algotradeplan.data.query import asset_sources_matrix  # type: ignore[import]
        return asset_sources_matrix(self._caps.map(), asset_classes)

    def discover_assets(self, source: str, limit: int = 10, **filters: Any) -> list[str]:
        cap = self._caps.get(source)
        if not cap or not cap.get("supports_discovery", False):
            return []
        if self._adapter_registry is None:
            return []
        try:
            symbols = self._adapter_registry.discover_assets(source, max(1, limit), **filters)
        except KeyError:
            return []
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

        raw_datasets: dict[str, Any] = {}
        if fetchable and self._adapter_registry is not None:
            try:
                raw_datasets = self._adapter_registry.fetch_raw(
                    source=source,
                    symbol=symbol,
                    datasets=fetchable,
                    timeframe=timeframe,
                    limit=limit,
                    **fetch_options,
                )
            except KeyError:
                pass
        adapter_issues = raw_datasets.pop("__issues__", {}) if isinstance(raw_datasets, dict) else {}
        if isinstance(adapter_issues, dict):
            for ds, reason in adapter_issues.items():
                source_issues_by_dataset[str(ds)] = str(reason)

        # Record all non-fetchable datasets as source issues.
        issues: list[dict[str, str]] = [
            {"source": source, "reason": reason}
            for ds, reason in source_issues_by_dataset.items()
        ]

        if not raw_datasets:
            # Nothing to send to Rust; short-circuit.
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
