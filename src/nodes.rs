// XrayClientCache: LRU pool of tonic gRPC channels keyed by (domain, port, token).
// NodeContext: axum extractor that parses X-Node-* headers and returns a cached client.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{extract::FromRequestParts, http::request::Parts};
use lru::LruCache;
use parking_lot::Mutex;
use tonic::transport::{Channel, ClientTlsConfig};

use crate::{config::Settings, error::AppError};

pub const DEFAULT_MAX_ENTRIES: usize = 256;

/// A single cached gRPC channel to an xray node.
#[derive(Clone)]
pub struct XrayClient {
    pub channel: Channel,
    /// Node API token sent as `x-api-token` gRPC metadata on every request.
    pub token: String,
    /// Diagnostic label (from X-Node-Name header, if provided).
    pub name: Option<String>,
}

/// LRU cache of XrayClient instances keyed by (domain, port, token).
/// Token is included in the key: different tokens = different clients.
pub struct XrayClientCache {
    inner: Mutex<LruCache<(String, u16, String), XrayClient>>,
}

impl XrayClientCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            inner: Mutex::new(LruCache::new(
                std::num::NonZeroUsize::new(max_entries).expect("max_entries must be > 0"),
            )),
        }
    }

    /// Return cached client or create a new one with a lazy gRPC channel.
    /// No `.await` while holding lock — channel construction is synchronous (lazy connect).
    pub fn get_or_create(
        &self,
        domain: &str,
        port: u16,
        token: &str,
        name: Option<&str>,
    ) -> Result<XrayClient, AppError> {
        let key = (domain.to_owned(), port, token.to_owned());
        let mut cache = self.inner.lock();

        if let Some(client) = cache.get(&key) {
            return Ok(client.clone());
        }

        // Capacity full: lru crate handles LRU eviction automatically when inserting.
        // We log which entry is being displaced when the cache is full.
        if cache.len() >= cache.cap().get() {
            if let Some((old_key, _)) = cache.peek_lru() {
                tracing::info!(
                    domain = %old_key.0,
                    port = old_key.1,
                    "evicting oldest XrayClient from cache"
                );
            }
        }

        let uri = format!("https://{domain}:{port}")
            .parse::<tonic::transport::Uri>()
            .map_err(|e| AppError::Internal(format!("invalid URI: {e}")))?;

        let tls_config = ClientTlsConfig::new().with_webpki_roots();

        let channel = Channel::builder(uri)
            .tls_config(tls_config)
            .map_err(|e| AppError::Internal(format!("TLS config error: {e}")))?
            .connect_lazy();

        let client = XrayClient {
            channel,
            token: token.to_owned(),
            name: name.map(str::to_owned),
        };

        tracing::debug!(
            domain = %domain,
            port = port,
            cached = cache.len() + 1,
            max = cache.cap().get(),
            "created new XrayClient"
        );

        cache.put(key, client.clone());
        Ok(client)
    }

    #[cfg(test)]
    pub fn size(&self) -> usize {
        self.inner.lock().len()
    }
}

/// Shared application state injected into all axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub cache: Arc<XrayClientCache>,
}

impl AppState {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings: Arc::new(settings),
            cache: Arc::new(XrayClientCache::new(DEFAULT_MAX_ENTRIES)),
        }
    }
}

/// Axum extractor: parses X-Node-* headers and returns a cached XrayClient.
/// Returns AppError::InvalidArgument if required headers are missing.
pub struct NodeContext(pub XrayClient);

#[async_trait]
impl FromRequestParts<AppState> for NodeContext {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let domain = parts
            .headers
            .get("x-node-domain")
            .and_then(|v| v.to_str().ok())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppError::InvalidArgument("缺少 X-Node-Domain header".to_owned()))?;

        let token = parts
            .headers
            .get("x-node-token")
            .and_then(|v| v.to_str().ok())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppError::InvalidArgument("缺少 X-Node-Token header".to_owned()))?;

        let port: u16 = parts
            .headers
            .get("x-node-port")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse().ok())
            .unwrap_or(443);

        let name = parts
            .headers
            .get("x-node-name")
            .and_then(|v| v.to_str().ok());

        tracing::debug!(
            domain = %domain,
            port = port,
            name = ?name,
            "node context resolved"
        );

        let client = state.cache.get_or_create(domain, port, token, name)?;
        Ok(NodeContext(client))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cache() -> XrayClientCache {
        XrayClientCache::new(3)
    }

    #[tokio::test]
    async fn cache_get_or_create_same_key_reuses_entry() {
        let cache = make_cache();
        let c1 = cache
            .get_or_create("node1.example.com", 443, "token1", None)
            .unwrap();
        let c2 = cache
            .get_or_create("node1.example.com", 443, "token1", None)
            .unwrap();
        assert_eq!(cache.size(), 1);
        // Both should point to the same token (proving same entry)
        assert_eq!(c1.token, c2.token);
    }

    #[tokio::test]
    async fn cache_different_tokens_are_different_entries() {
        let cache = make_cache();
        cache
            .get_or_create("node1.example.com", 443, "token-a", None)
            .unwrap();
        cache
            .get_or_create("node1.example.com", 443, "token-b", None)
            .unwrap();
        assert_eq!(cache.size(), 2);
    }

    #[tokio::test]
    async fn cache_lru_evicts_oldest_entry_when_full() {
        // capacity = 3; insert 4 distinct keys → first one gets evicted
        let cache = make_cache();
        cache
            .get_or_create("n1.example.com", 443, "t1", None)
            .unwrap();
        cache
            .get_or_create("n2.example.com", 443, "t2", None)
            .unwrap();
        cache
            .get_or_create("n3.example.com", 443, "t3", None)
            .unwrap();
        assert_eq!(cache.size(), 3);

        // Access n1 to make it recently used
        cache
            .get_or_create("n1.example.com", 443, "t1", None)
            .unwrap();

        // Insert n4 — this should evict n2 (least recently used)
        cache
            .get_or_create("n4.example.com", 443, "t4", None)
            .unwrap();
        assert_eq!(cache.size(), 3);

        // n1, n3, n4 should still be in cache
        {
            let mut inner = cache.inner.lock();
            assert!(inner
                .get(&("n1.example.com".to_owned(), 443, "t1".to_owned()))
                .is_some());
            assert!(inner
                .get(&("n2.example.com".to_owned(), 443, "t2".to_owned()))
                .is_none());
            assert!(inner
                .get(&("n3.example.com".to_owned(), 443, "t3".to_owned()))
                .is_some());
            assert!(inner
                .get(&("n4.example.com".to_owned(), 443, "t4".to_owned()))
                .is_some());
        }
    }

    #[tokio::test]
    async fn cache_put_get_basic() {
        let cache = make_cache();
        assert_eq!(cache.size(), 0);
        cache
            .get_or_create("a.example.com", 8443, "tok", Some("node-a"))
            .unwrap();
        assert_eq!(cache.size(), 1);
    }
}
