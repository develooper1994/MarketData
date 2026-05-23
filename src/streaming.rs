use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// A minimal message type produced by streaming adapters: (symbol, payload)
pub type StreamMessage = (String, Value);

/// Streaming adapters provide a background streaming capability for a source.
///
/// Implementations are expected to spawn background `tokio` tasks and manage
/// their own lifecycle; the methods are intentionally synchronous to keep the
/// registry integration simple. Implementations should return quickly after
/// scheduling work.
pub trait StreamingSourceAdapter: Send + Sync {
    /// Start streaming for the given `symbol` and datasets. Implementations
    /// should spawn tasks and begin emitting messages to their internal
    /// consumers (or external queues) and return Ok when the stream is active.
    fn start_stream(
        &self,
        symbol: &str,
        datasets: &[String],
    ) -> Result<(), crate::providers::errors::ProviderError>;

    /// Stop streaming for the given symbol. Implementations should stop any
    /// background tasks and release resources.
    fn stop_stream(&self, symbol: &str) -> Result<(), crate::providers::errors::ProviderError>;

    /// Optional discovery helper for streaming sources.
    fn discover_assets(&self, _limit: usize) -> Vec<String> {
        Vec::new()
    }
}

/// Registry for streaming adapters.
pub struct StreamingAdapterRegistry {
    adapters: HashMap<String, Arc<dyn StreamingSourceAdapter>>,
}

impl StreamingAdapterRegistry {
    pub fn register(
        &mut self,
        source: impl Into<String>,
        adapter: Arc<dyn StreamingSourceAdapter>,
    ) {
        self.adapters.insert(source.into(), adapter);
    }

    pub fn get(&self, source: &str) -> Option<Arc<dyn StreamingSourceAdapter>> {
        self.adapters.get(source).cloned()
    }
}

impl Default for StreamingAdapterRegistry {
    fn default() -> Self {
        Self {
            adapters: HashMap::new(),
        }
    }
}
