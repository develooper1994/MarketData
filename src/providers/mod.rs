pub mod errors;
pub mod yahoo;
pub mod btcturk;
pub mod kap;
pub mod fintables;
pub mod tradingview;
pub mod tradingview_ws;
pub mod paratic;
pub mod dovizcom;
#[cfg(feature = "tefas")]
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
    // TEFAS provider: compiled-in only when the `tefas` Cargo feature is enabled.
    #[cfg(feature = "tefas")]
    {
        // At runtime the provider may still be gated behind env vars or auto-registered
        // by the bridge when explicitly requested; here we register the adapter if
        // the feature is present so consumers can opt-in at build time.
        let tefas_adapter = Arc::new(tefas::TefasAdapter::default());
        registry.register("tefas", tefas_adapter.clone());
        // capability map uses `tefas_public`
        registry.register("tefas_public", tefas_adapter.clone());
    }

    #[cfg(not(feature = "tefas"))]
    {
        // When the feature is not compiled in, print a terse debug note if debugging is enabled.
        if std::env::var("TEFAS_DEBUG").map(|v| v == "1").unwrap_or(false) {
            eprintln!("TEFAS provider not compiled in (feature \"tefas\" not enabled)");
        }
    }
}
