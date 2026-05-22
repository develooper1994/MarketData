pub mod errors;
pub mod yahoo;
pub mod btcturk;
pub mod kap;
pub mod fintables;
pub mod tradingview;
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
    registry.register("paratic", Arc::new(paratic::ParaticAdapter::default()));
    registry.register("dovizcom", Arc::new(dovizcom::DovizComAdapter::default()));
    let tefas_adapter = Arc::new(tefas::TefasAdapter::default());
    registry.register("tefas", tefas_adapter.clone());
    // capability map uses `tefas_public`
    registry.register("tefas_public", tefas_adapter.clone());
}
