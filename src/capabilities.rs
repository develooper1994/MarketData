use serde::{Deserialize, Serialize};

/// Machine-readable metadata describing what a data source supports.
///
/// This mirrors `SourceCapability` from `AlgoTradePlan`'s `data/capabilities.py`
/// and is the authoritative source of that metadata inside `MarketData`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceCapability {
    pub source: String,
    pub asset_classes: Vec<String>,
    pub datasets: Vec<String>,
    pub supports_discovery: bool,
    pub supports_history: bool,
    pub supports_realtime: bool,
    #[serde(default)]
    pub requires_api_key: bool,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub rate_limit_notes: String,
    #[serde(default = "default_quality_community")]
    pub quality_level: String,
    #[serde(default)]
    pub implemented_datasets: Vec<String>,
    #[serde(default)]
    pub metadata_only_datasets: Vec<String>,
    #[serde(default = "default_status_live")]
    pub implementation_status: String,
    #[serde(default)]
    pub notes: String,
}

fn default_quality_community() -> String {
    "community".to_string()
}

fn default_status_live() -> String {
    "live".to_string()
}

/// Canonical dataset-name aliases (e.g. "ohlcv" → "kline").
pub const DATASET_ALIASES: &[(&str, &str)] = &[
    ("ohlcv", "kline"),
    ("candle", "kline"),
    ("candles", "kline"),
    ("ticker", "tick"),
    ("quote", "tick"),
    ("trades", "trade"),
    ("fills", "trade"),
    ("book", "orderbook"),
    ("order_book", "orderbook"),
    ("funding_rate", "funding"),
    ("headlines", "news"),
    ("macro_snapshot", "macro"),
    ("macro_series", "macro"),
    ("economic", "macro"),
    ("fundamental", "fundamentals"),
    ("corp_actions", "corporate_actions"),
    ("corporate_action", "corporate_actions"),
];

/// Resolve a raw dataset name to its canonical form.
pub fn canonical_dataset_name(dataset: &str) -> &str {
    DATASET_ALIASES
        .iter()
        .find(|(alias, _)| alias.eq_ignore_ascii_case(dataset))
        .map(|(_, canonical)| *canonical)
        .unwrap_or(dataset)
}

/// All known source capabilities.  This is the single authoritative copy;
/// AlgoTradePlan's `data/capabilities.py` should be deleted and replaced by a
/// query to this binary.
pub fn all_capabilities() -> Vec<SourceCapability> {
    vec![
        SourceCapability {
            source: "binance_futures".into(),
            asset_classes: vec!["crypto_perpetual".into()],
            datasets: vec!["tick".into(), "kline".into(), "trade".into(), "orderbook".into(), "funding".into()],
            supports_discovery: true,
            supports_history: true,
            supports_realtime: true,
            rate_limit_notes: "Public futures REST endpoints; respect exchange burst limits.".into(),
            quality_level: "production".into(),
            implemented_datasets: vec!["tick".into(), "kline".into(), "trade".into(), "orderbook".into(), "funding".into()],
            implementation_status: "live".into(),
            ..default_cap("binance_futures")
        },
        SourceCapability {
            source: "binance_spot".into(),
            asset_classes: vec!["crypto_spot".into()],
            datasets: vec!["tick".into(), "kline".into(), "trade".into(), "orderbook".into()],
            supports_discovery: true,
            supports_history: true,
            supports_realtime: true,
            rate_limit_notes: "Public spot REST endpoints; no funding dataset on spot market.".into(),
            quality_level: "production".into(),
            implemented_datasets: vec!["tick".into(), "kline".into(), "trade".into(), "orderbook".into()],
            implementation_status: "live".into(),
            ..default_cap("binance_spot")
        },
        SourceCapability {
            source: "bybit_linear".into(),
            asset_classes: vec!["crypto_perpetual".into()],
            datasets: vec!["tick".into(), "kline".into(), "trade".into(), "orderbook".into(), "funding".into()],
            supports_discovery: true,
            supports_history: true,
            supports_realtime: true,
            rate_limit_notes: "Public linear market endpoints; funding available.".into(),
            quality_level: "production".into(),
            implemented_datasets: vec!["tick".into(), "kline".into(), "trade".into(), "orderbook".into(), "funding".into()],
            implementation_status: "live".into(),
            ..default_cap("bybit_linear")
        },
        SourceCapability {
            source: "kraken_spot".into(),
            asset_classes: vec!["crypto_spot".into(), "forex".into()],
            datasets: vec!["tick".into(), "kline".into(), "trade".into(), "orderbook".into()],
            supports_discovery: true,
            supports_history: true,
            supports_realtime: false,
            rate_limit_notes: "Public spot endpoints; funding derived as unsupported.".into(),
            quality_level: "production".into(),
            implemented_datasets: vec!["tick".into(), "kline".into(), "trade".into(), "orderbook".into()],
            implementation_status: "live".into(),
            ..default_cap("kraken_spot")
        },
        SourceCapability {
            source: "coinbase_spot".into(),
            asset_classes: vec!["crypto_spot".into()],
            datasets: vec!["tick".into(), "kline".into(), "trade".into(), "orderbook".into()],
            supports_discovery: true,
            supports_history: true,
            supports_realtime: false,
            rate_limit_notes: "Public exchange endpoints; no funding feed.".into(),
            quality_level: "production".into(),
            implemented_datasets: vec!["tick".into(), "kline".into(), "trade".into(), "orderbook".into()],
            implementation_status: "live".into(),
            ..default_cap("coinbase_spot")
        },
        SourceCapability {
            source: "yahoo_unofficial".into(),
            asset_classes: vec!["crypto_spot".into(), "equity".into(), "etf".into(), "forex".into(), "index".into(), "options".into()],
            datasets: vec!["tick".into(), "kline".into()],
            supports_discovery: true,
            supports_history: true,
            supports_realtime: false,
            rate_limit_notes: "Unofficial Yahoo chart/search endpoints.".into(),
            quality_level: "best_effort".into(),
            implemented_datasets: vec!["tick".into(), "kline".into()],
            implementation_status: "partial".into(),
            notes: "Discovery + tick/kline available via unofficial chart/search responses; options coverage remains example-level metadata.".into(),
            ..default_cap("yahoo_unofficial")
        },
        SourceCapability {
            source: "alpha_vantage".into(),
            asset_classes: vec!["equity".into(), "etf".into(), "forex".into()],
            datasets: vec!["tick".into(), "kline".into(), "fundamentals".into()],
            supports_discovery: true,
            supports_history: true,
            supports_realtime: false,
            requires_api_key: true,
            api_key_env: Some("ALPHAVANTAGE_API_KEY".into()),
            rate_limit_notes: "Free tier is heavily rate limited.".into(),
            quality_level: "production".into(),
            implemented_datasets: vec!["tick".into(), "kline".into(), "fundamentals".into()],
            implementation_status: "api_key".into(),
            notes: "Fetches GLOBAL_QUOTE, intraday/daily kline, and Company Overview fundamentals when key is present.".into(),
            ..default_cap("alpha_vantage")
        },
        SourceCapability {
            source: "twelve_data".into(),
            asset_classes: vec!["equity".into(), "etf".into(), "forex".into(), "index".into(), "crypto_spot".into()],
            datasets: vec!["tick".into(), "kline".into()],
            supports_discovery: true,
            supports_history: true,
            supports_realtime: false,
            requires_api_key: true,
            api_key_env: Some("TWELVEDATA_API_KEY".into()),
            rate_limit_notes: "API-keyed intraday time series provider.".into(),
            quality_level: "production".into(),
            implemented_datasets: vec!["tick".into(), "kline".into()],
            implementation_status: "api_key".into(),
            ..default_cap("twelve_data")
        },
        SourceCapability {
            source: "polygon_io".into(),
            asset_classes: vec!["equity".into(), "etf".into(), "options".into(), "forex".into(), "crypto_spot".into()],
            datasets: vec!["tick".into(), "kline".into(), "trade".into(), "news".into(), "corporate_actions".into()],
            supports_discovery: true,
            supports_history: true,
            supports_realtime: true,
            requires_api_key: true,
            api_key_env: Some("POLYGON_API_KEY".into()),
            rate_limit_notes: "Plan-dependent market coverage.".into(),
            quality_level: "production".into(),
            implemented_datasets: vec!["tick".into(), "kline".into(), "trade".into(), "news".into(), "corporate_actions".into()],
            implementation_status: "api_key_or_plan".into(),
            notes: "Aggregates/quotes, news, splits; options richness plan-dependent.".into(),
            ..default_cap("polygon_io")
        },
        SourceCapability {
            source: "finnhub".into(),
            asset_classes: vec!["equity".into(), "etf".into(), "forex".into(), "crypto_spot".into()],
            datasets: vec!["tick".into(), "kline".into(), "news".into(), "fundamentals".into()],
            supports_discovery: true,
            supports_history: true,
            supports_realtime: true,
            requires_api_key: true,
            api_key_env: Some("FINNHUB_API_KEY".into()),
            rate_limit_notes: "Token-based provider with broad fundamentals/news coverage.".into(),
            quality_level: "production".into(),
            implemented_datasets: vec!["tick".into(), "kline".into(), "news".into(), "fundamentals".into()],
            implementation_status: "api_key".into(),
            notes: "Fetches quote/candles, company-news, and profile when token is present.".into(),
            ..default_cap("finnhub")
        },
        SourceCapability {
            source: "quandl".into(),
            asset_classes: vec!["futures".into(), "macro".into(), "equity".into()],
            datasets: vec!["kline".into(), "macro".into(), "fundamentals".into()],
            supports_discovery: true,
            supports_history: true,
            supports_realtime: false,
            requires_api_key: true,
            api_key_env: Some("QUANDL_API_KEY".into()),
            rate_limit_notes: "Historical and economic data only.".into(),
            quality_level: "production".into(),
            implemented_datasets: vec!["kline".into(), "macro".into()],
            metadata_only_datasets: vec!["fundamentals".into()],
            implementation_status: "api_key".into(),
            notes: "Historical futures/price series and macro-style snapshots.".into(),
        },
        SourceCapability {
            source: "iex_cloud".into(),
            asset_classes: vec!["equity".into(), "etf".into()],
            datasets: vec!["tick".into(), "kline".into(), "trade".into(), "news".into(), "corporate_actions".into()],
            supports_discovery: true,
            supports_history: true,
            supports_realtime: true,
            requires_api_key: true,
            api_key_env: Some("IEX_CLOUD_API_KEY".into()),
            rate_limit_notes: "Token required; plan-specific endpoints.".into(),
            quality_level: "production".into(),
            implemented_datasets: vec!["tick".into(), "kline".into(), "trade".into(), "news".into(), "corporate_actions".into()],
            implementation_status: "api_key".into(),
            notes: "Fetches quote/chart, news, dividend-style corporate actions when token is present.".into(),
            ..default_cap("iex_cloud")
        },
        SourceCapability {
            source: "frankfurter_fx".into(),
            asset_classes: vec!["forex".into(), "macro".into()],
            datasets: vec!["macro".into(), "tick".into()],
            supports_discovery: true,
            supports_history: true,
            supports_realtime: false,
            rate_limit_notes: "Public FX reference rates.".into(),
            quality_level: "production".into(),
            implemented_datasets: vec!["macro".into(), "tick".into()],
            implementation_status: "live".into(),
            ..default_cap("frankfurter_fx")
        },
        SourceCapability {
            source: "coingecko".into(),
            asset_classes: vec!["crypto_spot".into()],
            datasets: vec!["tick".into(), "kline".into(), "news".into()],
            supports_discovery: true,
            supports_history: true,
            supports_realtime: false,
            rate_limit_notes: "Public crypto market metadata and pricing.".into(),
            quality_level: "best_effort".into(),
            implemented_datasets: vec!["tick".into(), "kline".into()],
            metadata_only_datasets: vec!["news".into()],
            implementation_status: "partial".into(),
            notes: "OHLCV is synthesized from market_chart close/volume buckets.".into(),
            ..default_cap("coingecko")
        },
        SourceCapability {
            source: "stooq".into(),
            asset_classes: vec!["equity".into(), "etf".into(), "index".into(), "forex".into()],
            datasets: vec!["kline".into()],
            supports_discovery: true,
            supports_history: true,
            supports_realtime: false,
            rate_limit_notes: "End-of-day style market coverage.".into(),
            quality_level: "best_effort".into(),
            implemented_datasets: vec!["kline".into()],
            implementation_status: "live".into(),
            notes: "Timestamps are normalized from source dates to epoch-ms.".into(),
            ..default_cap("stooq")
        },
        SourceCapability {
            source: "fred".into(),
            asset_classes: vec!["macro".into()],
            datasets: vec!["macro".into()],
            supports_discovery: true,
            supports_history: true,
            supports_realtime: false,
            requires_api_key: true,
            api_key_env: Some("FRED_API_KEY".into()),
            rate_limit_notes: "Macro time series provider; API key recommended.".into(),
            quality_level: "production".into(),
            implemented_datasets: vec!["macro".into()],
            implementation_status: "api_key".into(),
            notes: "Provides macro series such as FEDFUNDS, CPIAUCSL, UNRATE, DGS10, and GDP.".into(),
            ..default_cap("fred")
        },
        SourceCapability {
            source: "gdelt".into(),
            asset_classes: vec!["news".into()],
            datasets: vec!["news".into()],
            supports_discovery: true,
            supports_history: true,
            supports_realtime: false,
            rate_limit_notes: "News/event metadata feed.".into(),
            quality_level: "best_effort".into(),
            implemented_datasets: vec!["news".into()],
            implementation_status: "live".into(),
            ..default_cap("gdelt")
        },
        SourceCapability {
            source: "financial_modeling_prep".into(),
            asset_classes: vec!["equity".into(), "etf".into(), "options".into()],
            datasets: vec!["tick".into(), "kline".into(), "fundamentals".into(), "corporate_actions".into(), "news".into()],
            supports_discovery: true,
            supports_history: true,
            supports_realtime: false,
            requires_api_key: true,
            api_key_env: Some("FMP_API_KEY".into()),
            rate_limit_notes: "Optional provider for fundamentals, news, and corporate actions.".into(),
            quality_level: "best_effort".into(),
            implemented_datasets: vec!["tick".into(), "kline".into(), "fundamentals".into(), "corporate_actions".into(), "news".into()],
            implementation_status: "api_key".into(),
            notes: "Endpoint scope depends on API plan.".into(),
            ..default_cap("financial_modeling_prep")
        },
        SourceCapability {
            source: "sec_edgar".into(),
            asset_classes: vec!["equity".into()],
            datasets: vec!["fundamentals".into(), "news".into(), "corporate_actions".into()],
            supports_discovery: true,
            supports_history: true,
            supports_realtime: false,
            rate_limit_notes: "Public filing metadata and disclosures.".into(),
            quality_level: "production".into(),
            implemented_datasets: vec!["fundamentals".into(), "news".into(), "corporate_actions".into()],
            implementation_status: "partial".into(),
            notes: "Uses public SEC ticker, submissions, and company facts endpoints.".into(),
            ..default_cap("sec_edgar")
        },
        SourceCapability {
            source: "world_bank".into(),
            asset_classes: vec!["macro".into()],
            datasets: vec!["macro".into()],
            supports_discovery: true,
            supports_history: true,
            supports_realtime: false,
            rate_limit_notes: "Global macro indicators and development statistics.".into(),
            quality_level: "production".into(),
            implemented_datasets: vec!["macro".into()],
            implementation_status: "live".into(),
            notes: "Pass `country=` to target WLD, TUR, USA, or other ISO/World Bank country codes.".into(),
            ..default_cap("world_bank")
        },
        SourceCapability {
            source: "ecb".into(),
            asset_classes: vec!["macro".into(), "forex".into()],
            datasets: vec!["macro".into(), "tick".into()],
            supports_discovery: true,
            supports_history: true,
            supports_realtime: false,
            rate_limit_notes: "ECB market and macro reference series.".into(),
            quality_level: "production".into(),
            implemented_datasets: vec!["tick".into(), "macro".into()],
            implementation_status: "live".into(),
            notes: "FX fetches parameterized by quote symbol such as USD, GBP, or JPY.".into(),
            ..default_cap("ecb")
        },
        SourceCapability {
            source: "defillama".into(),
            asset_classes: vec!["crypto_spot".into(), "macro".into()],
            datasets: vec!["fundamentals".into(), "macro".into(), "news".into()],
            supports_discovery: true,
            supports_history: true,
            supports_realtime: false,
            rate_limit_notes: "DeFi TVL and protocol metadata.".into(),
            quality_level: "best_effort".into(),
            implemented_datasets: vec!["macro".into(), "fundamentals".into()],
            metadata_only_datasets: vec!["news".into()],
            implementation_status: "partial".into(),
            notes: "Protocol catalog is cached in-process; macro tracks TVL time-series.".into(),
            ..default_cap("defillama")
        },
        SourceCapability {
            source: "hacker_news".into(),
            asset_classes: vec!["news".into()],
            datasets: vec!["news".into()],
            supports_discovery: true,
            supports_history: true,
            supports_realtime: false,
            rate_limit_notes: "Public story search used as a smoke-news source.".into(),
            quality_level: "best_effort".into(),
            implemented_datasets: vec!["news".into()],
            implementation_status: "live".into(),
            ..default_cap("hacker_news")
        },
        SourceCapability {
            source: "tefas_public".into(),
            asset_classes: vec!["mutual_fund".into(), "pension_fund".into()],
            datasets: vec![
                "fund_nav".into(), "fund_profile".into(), "fund_return".into(),
                "fund_allocation".into(), "fund_size".into(), "fund_fee".into(),
                "fund_announcement".into(), "fund_statistics".into(),
            ],
            supports_discovery: true,
            supports_history: true,
            supports_realtime: false,
            quality_level: "best_effort".into(),
            implemented_datasets: vec![
                "fund_nav".into(), "fund_profile".into(), "fund_return".into(),
                "fund_allocation".into(), "fund_size".into(), "fund_fee".into(),
                "fund_announcement".into(), "fund_statistics".into(),
            ],
            implementation_status: "partial".into(),
            notes: "Optional tefas-cli integration. Uses TEFAS public web JSON endpoints.".into(),
            ..default_cap("tefas_public")
        },
        SourceCapability {
            source: "offline_fallback".into(),
            asset_classes: vec!["crypto_perpetual".into()],
            datasets: vec!["tick".into(), "kline".into(), "trade".into(), "orderbook".into(), "funding".into()],
            supports_discovery: true,
            supports_history: true,
            supports_realtime: false,
            rate_limit_notes: "Deterministic fallback used only when live public sources are unreachable.".into(),
            quality_level: "fallback".into(),
            implemented_datasets: vec!["tick".into(), "kline".into(), "trade".into(), "orderbook".into(), "funding".into()],
            implementation_status: "fallback".into(),
            ..default_cap("offline_fallback")
        },
    ]
}

/// Build a source-name → capability lookup map.
pub fn capability_map() -> std::collections::HashMap<String, SourceCapability> {
    all_capabilities()
        .into_iter()
        .map(|c| (c.source.clone(), c))
        .collect()
}

fn default_cap(source: &str) -> SourceCapability {
    SourceCapability {
        source: source.to_string(),
        asset_classes: vec![],
        datasets: vec![],
        supports_discovery: false,
        supports_history: false,
        supports_realtime: false,
        requires_api_key: false,
        api_key_env: None,
        rate_limit_notes: String::new(),
        quality_level: "community".to_string(),
        implemented_datasets: vec![],
        metadata_only_datasets: vec![],
        implementation_status: "live".to_string(),
        notes: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_sources_present() {
        let caps = all_capabilities();
        let sources: Vec<&str> = caps.iter().map(|c| c.source.as_str()).collect();
        assert!(sources.contains(&"binance_futures"));
        assert!(sources.contains(&"tefas_public"));
        assert!(sources.contains(&"offline_fallback"));
        assert_eq!(caps.len(), 25);
    }

    #[test]
    fn capability_map_is_indexed_by_source() {
        let map = capability_map();
        assert!(map.contains_key("binance_futures"));
        assert_eq!(
            map["binance_futures"].asset_classes,
            vec!["crypto_perpetual"]
        );
    }

    #[test]
    fn canonical_dataset_name_resolves_aliases() {
        assert_eq!(canonical_dataset_name("ohlcv"), "kline");
        assert_eq!(canonical_dataset_name("candles"), "kline");
        assert_eq!(canonical_dataset_name("ticker"), "tick");
        assert_eq!(canonical_dataset_name("fills"), "trade");
        assert_eq!(canonical_dataset_name("order_book"), "orderbook");
        assert_eq!(canonical_dataset_name("funding_rate"), "funding");
        assert_eq!(canonical_dataset_name("headlines"), "news");
        assert_eq!(canonical_dataset_name("economic"), "macro");
        assert_eq!(canonical_dataset_name("fundamental"), "fundamentals");
        assert_eq!(canonical_dataset_name("corp_actions"), "corporate_actions");
        assert_eq!(canonical_dataset_name("kline"), "kline");
        assert_eq!(canonical_dataset_name("unknown_ds"), "unknown_ds");
    }
}
