"""Companion example for AlgoTradePlan integration.

This file is intentionally standalone so it can be copied into AlgoTradePlan.
"""

from __future__ import annotations

from typing import Any


class MarketDataBridge:
    """Placeholder bridge API for routing selected datasets to Rust MarketData."""

    def ingest(self, *, source: str, symbol: str, datasets: list[str], **fetch_options: Any) -> dict[str, Any]:
        # Integrate with the Rust layer here (FFI, service, or CLI bridge).
        return {
            "source": source,
            "symbol": symbol,
            "datasets": datasets,
            "fetch_options": fetch_options,
            "note": "Replace with MarketData Rust invocation.",
        }
