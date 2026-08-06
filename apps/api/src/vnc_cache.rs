use dashmap::DashMap;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct CacheEntry {
    pub status: String,
}

#[derive(Clone)]
pub struct VncCache {
    inner: Arc<DashMap<String, CacheEntry>>,
}

impl Default for VncCache {
    fn default() -> Self {
        Self::new()
    }
}

impl VncCache {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    pub fn insert(&self, access_token: &str, status: &str) {
        self.inner.insert(
            access_token.to_string(),
            CacheEntry {
                status: status.to_string(),
            },
        );
    }

    pub fn remove(&self, access_token: &str) {
        self.inner.remove(access_token);
    }

    pub fn get(&self, access_token: &str) -> Option<CacheEntry> {
        self.inner.get(access_token).map(|r| r.clone())
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_cache_is_empty() {
        let cache = VncCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn insert_and_get() {
        let cache = VncCache::new();
        cache.insert("tok1", "running");
        let entry = cache.get("tok1").expect("expected entry");
        assert_eq!(entry.status, "running");
    }

    #[test]
    fn get_missing_returns_none() {
        let cache = VncCache::new();
        assert!(cache.get("nope").is_none());
    }

    #[test]
    fn insert_overwrites() {
        let cache = VncCache::new();
        cache.insert("tok1", "starting");
        cache.insert("tok1", "running");
        assert_eq!(cache.get("tok1").unwrap().status, "running");
    }

    #[test]
    fn remove_deletes_entry() {
        let cache = VncCache::new();
        cache.insert("tok1", "running");
        assert_eq!(cache.len(), 1);
        cache.remove("tok1");
        assert!(cache.get("tok1").is_none());
        assert!(cache.is_empty());
    }

    #[test]
    fn remove_nonexistent_is_noop() {
        let cache = VncCache::new();
        cache.remove("nope");
        assert!(cache.is_empty());
    }

    #[test]
    fn multiple_entries() {
        let cache = VncCache::new();
        cache.insert("a", "running");
        cache.insert("b", "stopped");
        cache.insert("c", "running");
        assert_eq!(cache.len(), 3);
        assert_eq!(cache.get("a").unwrap().status, "running");
        assert_eq!(cache.get("b").unwrap().status, "stopped");
        assert_eq!(cache.get("c").unwrap().status, "running");
    }

    #[test]
    fn cache_is_cloneable_and_shared() {
        let cache = VncCache::new();
        let cache2 = cache.clone();
        cache.insert("tok1", "running");
        assert_eq!(cache2.get("tok1").unwrap().status, "running");
    }
}
