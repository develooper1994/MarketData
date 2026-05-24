use criterion::{criterion_group, criterion_main, Criterion};
use market_data::Etl;
use market_data::hub::DataHub;

fn etl_fetch_bench(c: &mut Criterion) {
    c.bench_function("etl_fetch_offline", |b| {
        b.iter(|| {
            // create a fresh Etl each iteration to avoid ownership issues
            let hub = DataHub::default();
            let etl = Etl::new(hub).source("offline").select_assets(vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()]);
            let _ = etl.fetch(vec!["kline".to_string()]);
        })
    });
}

criterion_group!(benches, etl_fetch_bench);
criterion_main!(benches);
