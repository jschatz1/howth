//! Shared daemon state.
//!
//! Holds the resolver cache, file watcher, package cache, build cache,
//! registry client, compiler backend, and test worker, coordinating
//! cache invalidation when files change.

use crate::cache::{DaemonBuildCache, DaemonPkgJsonCache, DaemonResolverCache};
use crate::test_worker::NodeTestWorker;
use crate::v8_test_worker::V8TestWorker;
use crate::watch::WatcherState;
use fastnode_core::compiler::{CompilerBackend, SwcBackend};
use fastnode_core::config::Channel;
use fastnode_core::pkg::{PackageCache, RegistryClient};
use std::sync::Arc;

/// Shared daemon state containing cache and watcher.
pub struct DaemonState {
    /// Resolver cache for import resolution.
    pub cache: Arc<DaemonResolverCache>,
    /// File watcher for cache invalidation.
    pub watcher: Arc<WatcherState>,
    /// Package cache for npm packages.
    pub pkg_cache: Arc<PackageCache>,
    /// Package.json parse cache for exports/imports resolution.
    pub pkg_json_cache: Arc<DaemonPkgJsonCache>,
    /// Build cache for incremental builds.
    pub build_cache: Arc<DaemonBuildCache>,
    /// Compiler backend for transpilation (v3.1).
    pub compiler: Arc<dyn CompilerBackend>,
    /// Shared registry client with persistent packument cache.
    pub registry: Arc<RegistryClient>,
    /// Warm Node.js test worker (lazy-started on first test run, fallback).
    pub test_worker: tokio::sync::Mutex<Option<NodeTestWorker>>,
    /// Native V8 test worker (lazy-started on first test run).
    pub v8_test_worker: std::sync::Mutex<Option<V8TestWorker>>,
}

// Manual Debug impl because dyn CompilerBackend doesn't implement Debug
impl std::fmt::Debug for DaemonState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DaemonState")
            .field("cache", &self.cache)
            .field("watcher", &self.watcher)
            .field("pkg_cache", &self.pkg_cache)
            .field("pkg_json_cache", &self.pkg_json_cache)
            .field("build_cache", &self.build_cache)
            .field("compiler", &self.compiler.name())
            .field("registry", &"RegistryClient")
            .field("test_worker", &"<Mutex>")
            .field("v8_test_worker", &"<Mutex>")
            .finish()
    }
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
        let build_cache = Arc::new(DaemonBuildCache::new());
        let compiler: Arc<dyn CompilerBackend> = Arc::new(SwcBackend::new());

        // Create shared registry client with persistent packument cache
        let registry =
            RegistryClient::from_env_with_cache((*pkg_cache).clone()).unwrap_or_else(|_| {
                RegistryClient::from_env().expect("Failed to create registry client")
            });

        Self {
            cache,
            watcher,
            pkg_cache,
            pkg_json_cache,
            build_cache,
            compiler,
            registry: Arc::new(registry),
            test_worker: tokio::sync::Mutex::new(None),
            v8_test_worker: std::sync::Mutex::new(None),
        }
    }

    /// Create daemon state with the given cache for invalidation.
    #[must_use]
    pub fn with_cache(cache: Arc<DaemonResolverCache>) -> Self {
        let watcher = Arc::new(WatcherState::new());
        let pkg_cache = Arc::new(PackageCache::new(Channel::Stable));
        let pkg_json_cache = Arc::new(DaemonPkgJsonCache::new());
        let build_cache = Arc::new(DaemonBuildCache::new());
        let compiler: Arc<dyn CompilerBackend> = Arc::new(SwcBackend::new());

        // Create shared registry client with persistent packument cache
        let registry =
            RegistryClient::from_env_with_cache((*pkg_cache).clone()).unwrap_or_else(|_| {
                RegistryClient::from_env().expect("Failed to create registry client")
            });

        Self {
            cache,
            watcher,
            pkg_cache,
            pkg_json_cache,
            build_cache,
            compiler,
            registry: Arc::new(registry),
            test_worker: tokio::sync::Mutex::new(None),
            v8_test_worker: std::sync::Mutex::new(None),
        }
    }

    /// Create daemon state with a custom compiler backend.
    #[must_use]
    pub fn with_compiler(compiler: Arc<dyn CompilerBackend>) -> Self {
        let cache = Arc::new(DaemonResolverCache::new());
        let watcher = Arc::new(WatcherState::new());
        let pkg_cache = Arc::new(PackageCache::new(Channel::Stable));
        let pkg_json_cache = Arc::new(DaemonPkgJsonCache::new());
        let build_cache = Arc::new(DaemonBuildCache::new());

        // Create shared registry client with persistent packument cache
        let registry =
            RegistryClient::from_env_with_cache((*pkg_cache).clone()).unwrap_or_else(|_| {
                RegistryClient::from_env().expect("Failed to create registry client")
            });

        Self {
            cache,
            watcher,
            pkg_cache,
            pkg_json_cache,
            build_cache,
            compiler,
            registry: Arc::new(registry),
            test_worker: tokio::sync::Mutex::new(None),
            v8_test_worker: std::sync::Mutex::new(None),
        }
    }
}

impl Default for DaemonState {
    fn default() -> Self {
        Self::new()
    }
}
