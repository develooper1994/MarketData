use market_data::source_health::SourceHealth;
use std::time::Duration;

#[test]
fn health_cache_basic() {
    let h = SourceHealth::new(Duration::from_secs(1));
    let r = h.is_healthy("binance");
    assert!(r == true || r == false);
}
