use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

pub struct SimpleCache<V> {
    inner: Mutex<HashMap<String, (V, SystemTime)>>,
    ttl: Duration,
}

impl<V> SimpleCache<V>
where
    V: Clone,
{
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            ttl: Duration::from_secs(ttl_seconds),
        }
    }

    pub fn get(&self, key: &str) -> Option<V> {
        let mut map = self.inner.lock().unwrap();
        if let Some((val, ts)) = map.get(key) {
            if ts.elapsed().unwrap_or(Duration::from_secs(0)) < self.ttl {
                return Some(val.clone());
            }
            map.remove(key);
        }
        None
    }

    pub fn set(&self, key: String, value: V) {
        let mut map = self.inner.lock().unwrap();
        map.insert(key, (value, SystemTime::now()));
    }
}
