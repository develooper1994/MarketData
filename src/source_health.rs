use crate::source_registry::SourceRegistry;
use once_cell::sync::Lazy;
use reqwest;
use std::collections::HashMap;
use std::env;
use std::sync::Mutex;
use std::time::{Duration, Instant};

static REGISTRY_PATH_OVERRIDE: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

struct HealthCacheEntry {
    healthy: bool,
    checked_at: Instant,
}

/// Basic in-memory source health checker with TTL-based caching.
/// This is a lightweight scaffold; `perform_check` is a placeholder
/// and should be implemented to actually probe the source endpoint.
pub struct SourceHealth {
    cache: Mutex<HashMap<String, HealthCacheEntry>>,
    ttl: Duration,
}

impl SourceHealth {
    pub fn new(ttl: Duration) -> Self {
        SourceHealth {
            cache: Mutex::new(HashMap::new()),
            ttl,
        }
    }

    /// Return whether the source is healthy, using cached value when fresh.
    pub fn is_healthy(&self, source_id: &str) -> bool {
        let mut cache = self.cache.lock().unwrap();
        if let Some(entry) = cache.get(source_id) {
            if entry.checked_at.elapsed() < self.ttl {
                return entry.healthy;
            }
        }

        let healthy = self.perform_check(source_id);
        cache.insert(
            source_id.to_string(),
            HealthCacheEntry {
                healthy,
                checked_at: Instant::now(),
            },
        );
        healthy
    }

    /// Placeholder check implementation. Replace with an actual probe.
    fn perform_check(&self, _source_id: &str) -> bool {
        // Attempt to load the source metadata to find a health_probe template.
        let registry_path = REGISTRY_PATH_OVERRIDE
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| {
                env::var("SOURCE_METADATA_PATH").unwrap_or_else(|_| {
                    format!("{}/config/source_metadata.yaml", env!("CARGO_MANIFEST_DIR"))
                })
            });

        let registry = match SourceRegistry::load_from_path(&registry_path) {
            Ok(r) => r,
            Err(_) => return true, // When registry not available, don't block ingestion
        };

        // Try exact match first, then substring match as a fallback
        let meta = registry.get(_source_id).cloned().or_else(|| {
            registry
                .all()
                .into_iter()
                .find(|m| m.id.contains(_source_id))
                .cloned()
        });

        let meta = match meta {
            Some(m) => m,
            None => return true,
        };

        // Allow overriding timeout in seconds via env for CI/testability
        let timeout_secs = env::var("SOURCE_HEALTH_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(3);

        let client = match reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .user_agent("market_data/1.0")
            .build()
        {
            Ok(c) => c,
            Err(_) => return true,
        };

        // Check for structured health probe in `api_templates.health` first
        if let Some(api_templates) = &meta.api_templates {
            if let Some(health_spec) = api_templates.get("health") {
                // parse fields
                let method = health_spec
                    .get("method")
                    .and_then(|v| v.as_str())
                    .unwrap_or("HEAD")
                    .to_uppercase();

                // endpoint may contain placeholders like {source}
                let mut endpoint = health_spec
                    .get("endpoint")
                    .and_then(|v| v.as_str())
                    .map(|s| s.replace("{source}", _source_id))
                    .unwrap_or_default();

                // if params provided, append as query string
                if let Some(params) = health_spec.get("params").and_then(|v| v.as_object()) {
                    if !params.is_empty() {
                        let mut q: Vec<String> = Vec::new();
                        for (k, val) in params.iter() {
                            if let Some(vs) = val.as_str() {
                                q.push(format!("{}={}", k, vs));
                            } else {
                                q.push(format!("{}={}", k, val.to_string()));
                            }
                        }
                        if endpoint.contains('?') {
                            endpoint = format!("{}&{}", endpoint, q.join("&"));
                        } else {
                            endpoint = format!("{}?{}", endpoint, q.join("&"));
                        }
                    }
                }

                // build request
                let mut req_builder = match method.as_str() {
                    "HEAD" => client.head(&endpoint),
                    "GET" => client.get(&endpoint),
                    "POST" => client.post(&endpoint),
                    "PUT" => client.put(&endpoint),
                    _ => client.get(&endpoint),
                };

                // headers
                if let Some(headers) = health_spec.get("headers").and_then(|v| v.as_object()) {
                    for (hk, hv) in headers.iter() {
                        if let Some(hs) = hv.as_str() {
                            // allow env var interpolation: {env:NAME}
                            let hv_final = if hs.starts_with("{env:") && hs.ends_with('}') {
                                let env_name = &hs[5..hs.len() - 1];
                                std::env::var(env_name).unwrap_or_default()
                            } else {
                                hs.to_string()
                            };
                            req_builder = req_builder.header(hk, hv_final);
                        }
                    }
                }

                // Allow SOURCE_HEALTH_AUTH_<SOURCE> override
                let auth_env = format!(
                    "SOURCE_HEALTH_AUTH_{}",
                    _source_id.to_uppercase().replace('-', "_")
                );
                if let Ok(auth_val) = std::env::var(&auth_env) {
                    req_builder = req_builder.header("Authorization", auth_val);
                }

                // optional body
                if let Some(body) = health_spec.get("body").and_then(|v| v.as_str()) {
                    let body_final = body.replace("{source}", _source_id);
                    req_builder = req_builder.body(body_final);
                }

                // execute request
                match req_builder.send() {
                    Ok(resp) => {
                        // expected_status may be int or array
                        if let Some(exp) = health_spec.get("expected_status") {
                            if exp.is_number() {
                                if resp.status().as_u16() == exp.as_u64().unwrap_or(0) as u16 {
                                    return true;
                                } else {
                                    return false;
                                }
                            } else if exp.is_array() {
                                let mut matched = false;
                                for v in exp.as_array().unwrap() {
                                    if v.is_number()
                                        && resp.status().as_u16() == v.as_u64().unwrap_or(0) as u16
                                    {
                                        matched = true;
                                        break;
                                    }
                                }
                                return matched;
                            }
                        }

                        // optional body_contains check
                        if let Some(body_contains) =
                            health_spec.get("body_contains").and_then(|v| v.as_str())
                        {
                            if let Ok(text) = resp.text() {
                                return text.contains(body_contains);
                            } else {
                                return false;
                            }
                        }

                        return resp.status().is_success();
                    }
                    Err(_) => return false,
                }
            }
        }

        // Fallback: legacy simple health_probe string (URL). Try HEAD then GET with retries.
        let probe = match meta.health_probe {
            Some(p) => p.replace("{source}", _source_id),
            None => return true,
        };

        // Try HEAD first (lightweight), fall back to GET when HEAD is not allowed
        let try_head = client.head(&probe).send();
        match try_head {
            Ok(resp) => {
                if resp.status().is_success() {
                    return true;
                }
                // If server reports HEAD not allowed, try GET below
                if resp.status().as_u16() == 405 || resp.status().as_u16() == 501 {
                    // fall through to GET
                } else {
                    return false;
                }
            }
            Err(_) => {
                // attempt GET as a fallback on error
            }
        }

        // GET fallback with one retry for transient errors
        for _attempt in 0..2 {
            match client.get(&probe).send() {
                Ok(resp) => return resp.status().is_success(),
                Err(_) => continue,
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::SourceHealth;
    use std::time::Duration;

    #[test]
    fn perform_check_uses_health_probe() {
        use httpmock::MockServer;
        use tempfile::NamedTempFile;

        // Start a mock HTTP server that responds 200 to /health
        let server = MockServer::start();
        let _m_get = server.mock(|when, then| {
            when.method("GET").path("/health");
            then.status(200).body("ok");
        });
        let _m_head = server.mock(|when, then| {
            when.method("HEAD").path("/health");
            then.status(200).body("");
        });

        // Create a temporary registry file pointing the source to the mock server
        let mut tmp = NamedTempFile::new().unwrap();
        let yaml = format!(
            "sources:\n  - id: \"mocksource\"\n    health_probe: \"{}\"\n    supported_asset_classes: []\n    supported_datasets: []\n",
            server.url("/health")
        );
        std::fs::write(tmp.path(), yaml).unwrap();

        // Point the loader at our temp registry via override (avoid env::set_var in tests)
        {
            let mut guard = super::REGISTRY_PATH_OVERRIDE.lock().unwrap();
            *guard = Some(tmp.path().to_str().unwrap().to_string());
        }

        let h = SourceHealth::new(Duration::from_secs(1));
        let r = h.is_healthy("mocksource");
        assert!(r, "expected health probe to return healthy status");

        // Clear override
        {
            let mut guard = super::REGISTRY_PATH_OVERRIDE.lock().unwrap();
            *guard = None;
        }
    }

    #[test]
    fn basic_health_cache_returns_bool() {
        let h = SourceHealth::new(Duration::from_secs(1));
        let r1 = h.is_healthy("binance");
        let r2 = h.is_healthy("binance");
        assert_eq!(r1, r2);
    }
}
