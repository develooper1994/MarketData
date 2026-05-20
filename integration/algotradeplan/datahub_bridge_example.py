"""Companion example for AlgoTradePlan integration.

This file is intentionally standalone so it can be copied into AlgoTradePlan.
"""

from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path
from typing import Any, Callable

try:  # Replace with the local project import path when copying into AlgoTradePlan.
    from src.algotradeplan.data.hub import IngestResult
    from src.algotradeplan.plugins.data.contracts import (
        DataRecord,
        DataRequest,
        ProvenanceRecord,
        QualityReport,
        StorageReceipt,
    )
except Exception:  # pragma: no cover - standalone example fallback
    IngestResult = None
    DataRecord = DataRequest = ProvenanceRecord = QualityReport = StorageReceipt = None


class MarketDataBridge:
    """Bridge selected ingestion work to the Rust MarketData binary.

    The intended integration point is inside ``AlgoTradePlan``'s existing
    ``DataHub`` implementation: keep capability checks and raw fetching in
    Python, then send the fetched dataset payloads to Rust for normalization,
    quality validation, storage, and provenance.
    """

    def __init__(
        self,
        *,
        binary: str | None = None,
        repo_root: str | os.PathLike[str] | None = None,
        raw_fetcher: Callable[..., dict[str, Any]] | None = None,
    ) -> None:
        self._binary = binary or os.getenv("MARKET_DATA_BIN")
        self._repo_root = Path(repo_root or os.getenv("MARKET_DATA_REPO", ".")).resolve()
        self._raw_fetcher = raw_fetcher

    def verify_setup(self) -> dict[str, Any]:
        completed = self._run(["doctor"])
        return json.loads(completed.stdout)

    def ingest(
        self,
        *,
        source: str,
        symbol: str,
        datasets: list[str],
        raw_datasets: dict[str, Any] | None = None,
        asset_type: str = "multi_asset",
        store: bool = True,
        **fetch_options: Any,
    ) -> Any:
        payload = raw_datasets
        if payload is None:
            if self._raw_fetcher is None:
                msg = "raw_datasets or raw_fetcher is required for Rust bridge ingestion"
                raise RuntimeError(msg)
            payload = self._raw_fetcher(
                source=source,
                symbol=symbol,
                datasets=datasets,
                timeframe=fetch_options.pop("timeframe", "1m"),
                limit=fetch_options.pop("limit", 500),
                **fetch_options,
            )

        command = [
            "ingest",
            "--source",
            source,
            "--symbol",
            symbol,
            "--datasets",
            ",".join(datasets),
            "--asset-type",
            asset_type,
        ]
        if store:
            command.append("--store")
            if record_root := fetch_options.get("record_root"):
                command.extend(["--record-root", str(record_root)])
            if manifest_root := fetch_options.get("manifest_root"):
                command.extend(["--manifest-root", str(manifest_root)])

        completed = self._run(command, input=json.dumps(payload))
        response = json.loads(completed.stdout)
        return _to_ingest_result(response)

    def _run(self, command: list[str], *, input: str = "") -> subprocess.CompletedProcess[str]:
        if self._binary:
            full_command = [self._binary, *command]
            cwd = None
        else:
            full_command = ["cargo", "run", "--quiet", "--bin", "market_data_bridge", "--", *command]
            cwd = self._repo_root
        return subprocess.run(
            full_command,
            cwd=cwd,
            input=input,
            text=True,
            capture_output=True,
            check=True,
        )


def _to_ingest_result(payload: dict[str, Any]) -> Any:
    if IngestResult is None:
        return payload
    quality_report = QualityReport(**payload["quality_report"])
    storage_receipts = [StorageReceipt(**item) for item in payload["storage_receipts"]]
    provenance = None
    if payload.get("provenance"):
        provenance_payload = payload["provenance"]
        provenance = ProvenanceRecord(
            request=DataRequest(**provenance_payload["request"]),
            source_plugin_id=provenance_payload["source_plugin_id"],
            storage_receipts=[StorageReceipt(**item) for item in provenance_payload["storage_receipts"]],
            record_keys=provenance_payload["record_keys"],
            revision=provenance_payload["revision"],
        )
    return IngestResult(
        source=payload["source"],
        symbol=payload["symbol"],
        requested_datasets=payload["requested_datasets"],
        dataset_coverage=payload["dataset_coverage"],
        raw_datasets=payload["raw_datasets"],
        normalized=payload["normalized"],
        records=[DataRecord(**item) for item in payload["records"]],
        quality_report=quality_report,
        storage_receipts=storage_receipts,
        provenance=provenance,
        source_issues=payload["source_issues"],
    )
