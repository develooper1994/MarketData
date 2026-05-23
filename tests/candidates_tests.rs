use market_data::candidates::generate_candidates;

#[test]
fn generate_eur_candidates_contains_eurusd() {
    let c = generate_candidates("EUR", Some("forex"));
    assert!(c.contains(&"EURUSD".to_string()));
}

#[test]
fn generate_btc_candidates_contains_btcusdt() {
    let c = generate_candidates("BTC", Some("crypto"));
    assert!(c.contains(&"BTCUSDT".to_string()));
}

#[test]
fn generate_from_eurusd_has_slash_or_plain() {
    let c = generate_candidates("EURUSD", Some("forex"));
    assert!(c.contains(&"EUR/USD".to_string()) || c.contains(&"EURUSD".to_string()));
}
