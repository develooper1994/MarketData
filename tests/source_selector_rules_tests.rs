use market_data::heuristics;

#[test]
fn detect_forex_eurusd() {
    assert_eq!(heuristics::detect_asset_type("EURUSD"), "forex");
}

#[test]
fn detect_forex_with_slash() {
    assert_eq!(heuristics::detect_asset_type("EUR/USD"), "forex");
}

#[test]
fn detect_crypto_btcusdt() {
    assert_eq!(heuristics::detect_asset_type("BTCUSDT"), "crypto");
}

#[test]
fn detect_fund_trfund() {
    assert_eq!(heuristics::detect_asset_type("TRFUND001"), "funds");
}

#[test]
fn detect_equity_aapl() {
    assert_eq!(heuristics::detect_asset_type("AAPL"), "equities");
}
