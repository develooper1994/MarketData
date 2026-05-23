use market_data::SourceRegistry;
use market_data::source_selector::SourceSelector;

#[test]
fn selector_prefers_registry_over_heuristic() {
    // Create a temporary registry that maps funds->tefas and equities->yahoo
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let yaml = r#"
sources:
  - id: "tefas"
    supported_asset_classes: ["funds"]
    supported_datasets: ["fundamentals"]
  - id: "yahoo"
    supported_asset_classes: ["equities"]
    supported_datasets: ["fundamentals"]
"#;
    std::fs::write(tmp.path(), yaml).unwrap();

    let registry = SourceRegistry::load_from_path(tmp.path().to_str().unwrap()).unwrap();
    let sel = SourceSelector::select("AAPL", "fundamentals", &registry, None, None, false, false);
    assert_eq!(sel.chosen.unwrap(), "yahoo");
}

#[test]
fn selector_selects_tefas_for_fund() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let yaml = r#"
sources:
  - id: "tefas"
    supported_asset_classes: ["funds"]
    supported_datasets: ["fundamentals"]
"#;
    std::fs::write(tmp.path(), yaml).unwrap();
    let registry = SourceRegistry::load_from_path(tmp.path().to_str().unwrap()).unwrap();
    let sel = SourceSelector::select(
        "TRFUND001",
        "fundamentals",
        &registry,
        None,
        None,
        false,
        false,
    );
    assert_eq!(sel.chosen.unwrap(), "tefas");
}

#[test]
fn selector_selects_binance_for_btcusdt_kline() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let yaml = r#"
sources:
  - id: "binance"
    supported_asset_classes: ["crypto"]
    supported_datasets: ["kline"]
"#;
    std::fs::write(tmp.path(), yaml).unwrap();
    let registry = SourceRegistry::load_from_path(tmp.path().to_str().unwrap()).unwrap();
    let sel = SourceSelector::select("BTCUSDT", "kline", &registry, None, None, false, false);
    assert_eq!(sel.chosen.unwrap(), "binance");
}

#[test]
fn selector_selects_forex_for_eurusd() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let yaml = r#"
sources:
  - id: "doviz"
    supported_asset_classes: ["forex"]
    supported_datasets: ["kline"]
"#;
    std::fs::write(tmp.path(), yaml).unwrap();
    let registry = SourceRegistry::load_from_path(tmp.path().to_str().unwrap()).unwrap();
    let sel = SourceSelector::select("EURUSD", "kline", &registry, None, None, false, false);
    assert_eq!(sel.chosen.unwrap(), "doviz");
}
