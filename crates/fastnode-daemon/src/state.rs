//! Shared daemon state.
//!
//! Holds the resolver cache, file watcher, and package cache, coordinating
//! cache invalidation when files change.

use crate::cache::{DaemonPkgJsonCache, DaemonResolverCache};
use crate::watch::WatcherState;
use fastnode_core::config::Channel;
use fastnode_core::pkg::PackageCache;
use std::sync::Arc;

/// Shared daemon state containing cache and watcher.
#[derive(Debug)]
pub struct DaemonState {
    /// Resolver cache for import resolution.
    pub cache: Arc<DaemonResolverCache>,
    /// File watcher for cache invalidation.
    pub watcher: Arc<WatcherState>,
    /// Package cache for npm packages.
    pub pkg_cache: Arc<PackageCache>,
    /// Package.json parse cache for exports/imports resolution.
    pub pkg_json_cache: Arc<DaemonPkgJsonCache>,
}

impl DaemonState {
    /// Create new daemon state with empty cache and stopped watcher.
    #[must_use]
    pub fn new() -> Self {
        Self::with_channel(Channel::Stable)
    }

    /// Create new daemon state with the given channel.
    #[must_use]
    pub fn with_channel(channel: Channel) -> Self {
        let cache = Arc::new(DaemonResolverCache::new());
        let watcher = Arc::new(WatcherState::new());
        let pkg_cache = Arc::new(PackageCache::new(channel));
        let pkg_json_cache = Arc::new(DaemonPkgJsonCache::new());
        Self {
            cache,
            watcher,
            pkg_cache,
            pkg_json_cache,
        }
    }

    /// Create daemon state with the given cache for invalidation.
    #[must_use]
    pub fn with_cache(cache: Arc<DaemonResolverCache>) -> Self {
        let watcher = Arc::new(WatcherState::new());
        let pkg_cache = Arc::new(PackageCache::new(Channel::Stable));
        let pkg_json_cache = Arc::new(DaemonPkgJsonCache::new());
        Self {
            cache,
            watcher,
            pkg_cache,
            pkg_json_cache,
        }
    }
}

impl Default for DaemonState {
    fn default() -> Self {
        Self::new()
    }
}
