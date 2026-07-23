use dashmap::DashMap;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct CacheEntry {
    pub status: String,
    #[allow(dead_code)]
    pub owner_id: Uuid,
}

#[derive(Clone)]
pub struct VncCache {
    inner: Arc<DashMap<String, CacheEntry>>,
}

impl VncCache {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    pub fn insert(&self, vnc_token: &str, status: &str, owner_id: Uuid) {
        self.inner.insert(
            vnc_token.to_string(),
            CacheEntry {
                status: status.to_string(),
                owner_id,
            },
        );
    }

    pub fn remove(&self, vnc_token: &str) {
        self.inner.remove(vnc_token);
    }

    pub fn get(&self, vnc_token: &str) -> Option<CacheEntry> {
        self.inner.get(vnc_token).map(|r| r.clone())
    }

    #[allow(dead_code)]
    pub fn is_running(&self, vnc_token: &str) -> Option<bool> {
        self.inner
            .get(vnc_token)
            .map(|r| r.status == "running")
    }
}
