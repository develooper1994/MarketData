pub mod errors;
pub mod yahoo;
pub mod btcturk;
pub mod kap;
pub mod fintables;
pub mod tradingview;
pub mod tradingview_ws;
pub mod paratic;
pub mod dovizcom;
pub mod tefas;

use crate::hub::SourceAdapterRegistry;
use std::sync::Arc;

pub fn register_live_providers(registry: &mut SourceAdapterRegistry) {
    // Register commonly-used live providers under short names
    let yahoo_adapter = Arc::new(yahoo::YahooAdapter::default());
    registry.register("yahoo", yahoo_adapter.clone());
    // capability map uses `yahoo_unofficial` as the canonical source name
    registry.register("yahoo_unofficial", yahoo_adapter.clone());
    registry.register("btcturk", Arc::new(btcturk::BtcturkAdapter::default()));
    registry.register("kap", Arc::new(kap::KapAdapter::default()));
    registry.register("fintables", Arc::new(fintables::FintablesAdapter::default()));
    // New scaffolds (minimal implementations)
    registry.register("tradingview", Arc::new(tradingview::TradingViewAdapter::default()));
    // Streaming adapter (POC)
    // Note: `tradingview_ws` provides a streaming POC that emits synthetic ticks when no
    // real websocket URL is configured via `TRADINGVIEW_WS_URL`.
    registry.register("paratic", Arc::new(paratic::ParaticAdapter::default()));
    registry.register("dovizcom", Arc::new(dovizcom::DovizComAdapter::default()));
    // TEFAS is opt-in via the `ENABLE_TEFAS` env var to avoid accidental live calls.
    if std::env::var("ENABLE_TEFAS").map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false) {
        let tefas_adapter = Arc::new(tefas::TefasAdapter::default());
        registry.register("tefas", tefas_adapter.clone());
        // capability map uses `tefas_public`
        registry.register("tefas_public", tefas_adapter.clone());
    } else {
        // Helpful debug message when explicitly requested
        if std::env::var("TEFAS_DEBUG").map(|v| v == "1").unwrap_or(false) {
            eprintln!("TEFAS provider not registered (ENABLE_TEFAS not set)");
        }
    }
}
