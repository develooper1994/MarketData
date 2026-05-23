use market_data::hub::SourceAdapterRegistry;

#[test]
fn register_live_providers_populates_registry() {
    let mut registry = SourceAdapterRegistry::default();
    // Register live providers without performing network calls
    market_data::providers::register_live_providers(&mut registry);

    assert!(
        registry.get("yahoo").is_some(),
        "yahoo provider should be registered"
    );
    assert!(
        registry.get("btcturk").is_some(),
        "btcturk provider should be registered"
    );
    assert!(
        registry.get("kap").is_some(),
        "kap provider should be registered"
    );
    assert!(
        registry.get("fintables").is_some(),
        "fintables provider should be registered"
    );
}
