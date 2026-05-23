use market_data::source_registry::SourceRegistry;

#[test]
fn load_registry_from_config() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(manifest_dir).join("config/source_metadata.yaml");
    let registry = SourceRegistry::load_from_path(path.to_str().unwrap()).expect("load registry");
    assert!(
        registry.get_by_asset_class("crypto").len() >= 1,
        "expected at least one crypto source"
    );
    assert!(
        registry.get("binance").is_some(),
        "expected 'binance' source present"
    );
    assert!(
        registry.all().len() >= 3,
        "expected at least 3 sources in sample metadata"
    );
}
