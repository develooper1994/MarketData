use crate::providers::errors::ProviderError;
use crate::streaming::StreamingSourceAdapter;
use rand::Rng;
use serde_json::json;
use std::collections::HashMap;
use std::fs::{OpenOptions, create_dir_all};
use std::io::Write;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::JoinHandle;
use std::time::Duration;

#[derive(Default)]
pub struct TradingViewStreamingAdapter {
    handles: Mutex<HashMap<String, (JoinHandle<()>, Arc<AtomicBool>)>>,
}

impl TradingViewStreamingAdapter {
    pub fn new() -> Self {
        Self::default()
    }
}

impl StreamingSourceAdapter for TradingViewStreamingAdapter {
    fn start_stream(&self, symbol: &str, _datasets: &[String]) -> Result<(), ProviderError> {
        let key = symbol.to_string();
        let mut handles = self.handles.lock().unwrap();
        if handles.contains_key(&key) {
            return Ok(());
        }

        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();
        let symbol_clone = key.clone();

        let handle = std::thread::spawn(move || {
            // Synthetic tick generator: writes JSON lines to artifacts/streams
            let mut rng = rand::thread_rng();
            let mut price: f64 = 100.0 + rng.gen_range(-5.0..5.0);
            let out_dir = std::path::Path::new("artifacts/streams");
            if let Err(e) = create_dir_all(out_dir) {
                eprintln!("tradingview_ws: failed to create artifacts dir: {}", e);
                return;
            }
            let out_file = out_dir.join(format!("tradingview_{}.jsonl", symbol_clone));

            while !stop_clone.load(Ordering::Relaxed) {
                // small random walk
                let step = rng.gen_range(-0.001..0.001);
                price = (price * (1.0 + step)).max(0.0001);
                let payload = json!([{
                    "timestamp_ms": chrono::Utc::now().timestamp_millis(),
                    "last": format!("{:.6}", price),
                    "bid": format!("{:.6}", price - 0.0005),
                    "ask": format!("{:.6}", price + 0.0005)
                }]);

                match OpenOptions::new().create(true).append(true).open(&out_file) {
                    Ok(mut f) => {
                        if let Ok(line) = serde_json::to_string(&payload) {
                            if let Err(e) = writeln!(f, "{}", line) {
                                eprintln!("tradingview_ws: write error: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("tradingview_ws: failed to open file: {}", e);
                    }
                }

                std::thread::sleep(Duration::from_secs(1));
            }
        });

        handles.insert(key, (handle, stop));
        Ok(())
    }

    fn stop_stream(&self, symbol: &str) -> Result<(), ProviderError> {
        let mut handles = self.handles.lock().unwrap();
        if let Some((handle, stop_flag)) = handles.remove(symbol) {
            stop_flag.store(true, Ordering::Relaxed);
            if let Err(e) = handle.join() {
                return Err(ProviderError::Other(format!(
                    "failed to join stream thread: {:?}",
                    e
                )));
            }
        }
        Ok(())
    }

    fn discover_assets(&self, limit: usize) -> Vec<String> {
        let samples = vec!["AAPL", "MSFT", "BTCUSD", "ETHUSD"]
            .into_iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        samples.into_iter().take(limit).collect()
    }
}
