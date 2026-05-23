use market_data::{source_registry::SourceRegistry, source_selector::SourceSelector};

#[test]
fn select_binance_for_btcusdt_kline() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(manifest_dir).join("config/source_metadata.yaml");
    let registry = SourceRegistry::load_from_path(path.to_str().unwrap()).expect("load registry");
    let res = SourceSelector::select("BTCUSDT", "kline", &registry, None, None, false, false);
    assert!(res.chosen.is_some());
    assert_eq!(res.chosen.unwrap(), "binance");
}
