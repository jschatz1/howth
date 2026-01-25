//! File watcher for cache invalidation.
//!
//! Watches directories for file changes and invalidates resolver cache entries.

use crate::cache::{DaemonBuildCache, DaemonPkgJsonCache, DaemonResolverCache};
use notify::{
    event::{CreateKind, ModifyKind, RemoveKind, RenameMode},
    Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher,
};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Event coalescing window.
const COALESCE_WINDOW_MS: u64 = 50;

/// Watcher state.
#[derive(Debug)]
pub struct WatcherState {
    /// Root directories being watched.
    roots: RwLock<Vec<String>>,
    /// Whether the watcher is running.
    running: AtomicBool,
    /// Timestamp of last invalidation event (ms since Unix epoch).
    /// Updated AFTER invalidation is applied.
    last_event_unix_ms: Arc<AtomicU64>,
    /// The actual watcher handle (when running).
    watcher: Mutex<Option<RecommendedWatcher>>,
    /// Event sender for async processing.
    event_tx: Mutex<Option<mpsc::UnboundedSender<WatchEvent>>>,
    /// Optional reference to resolver cache for invalidation.
    cache: Mutex<Option<Arc<DaemonResolverCache>>>,
    /// Optional reference to package.json cache for invalidation.
    pkg_json_cache: Mutex<Option<Arc<DaemonPkgJsonCache>>>,
    /// Optional reference to build cache for invalidation.
    build_cache: Mutex<Option<Arc<DaemonBuildCache>>>,
    /// Build watch subscribers (v3.0): directory path -> notification senders.
    build_watchers: Arc<Mutex<Vec<(PathBuf, mpsc::Sender<()>)>>>,
}

/// Watcher event for internal processing.
#[derive(Debug, Clone)]
pub struct WatchEvent {
    /// Paths that changed.
    pub paths: Vec<PathBuf>,
    /// Kind of change.
    pub kind: WatchEventKind,
}

/// Kind of watch event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchEventKind {
    Create,
    Modify,
    Remove,
    Rename,
    Other,
}

impl From<&EventKind> for WatchEventKind {
    fn from(kind: &EventKind) -> Self {
        match kind {
            EventKind::Create(_) => Self::Create,
            EventKind::Modify(_) => Self::Modify,
            EventKind::Remove(_) => Self::Remove,
            EventKind::Other => Self::Other,
            _ => Self::Other,
        }
    }
}

impl Default for WatcherState {
    fn default() -> Self {
        Self::new()
    }
}

impl WatcherState {
    /// Create a new watcher state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            roots: RwLock::new(Vec::new()),
            running: AtomicBool::new(false),
            last_event_unix_ms: Arc::new(AtomicU64::new(0)),
            watcher: Mutex::new(None),
            event_tx: Mutex::new(None),
            cache: Mutex::new(None),
            pkg_json_cache: Mutex::new(None),
            build_cache: Mutex::new(None),
            build_watchers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Set the cache to use for invalidation.
    pub fn set_cache(&self, cache: Arc<DaemonResolverCache>) {
        *self.cache.lock().unwrap() = Some(cache);
    }

    /// Set the package.json cache to use for invalidation.
    pub fn set_pkg_json_cache(&self, cache: Arc<DaemonPkgJsonCache>) {
        *self.pkg_json_cache.lock().unwrap() = Some(cache);
    }

    /// Set the build cache to use for invalidation.
    pub fn set_build_cache(&self, cache: Arc<DaemonBuildCache>) {
        *self.build_cache.lock().unwrap() = Some(cache);
    }

    /// Check if the watcher is running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Get the roots being watched.
    #[must_use]
    pub fn roots(&self) -> Vec<String> {
        self.roots.read().unwrap().clone()
    }

    /// Get the last event timestamp.
    #[must_use]
    pub fn last_event_unix_ms(&self) -> Option<u64> {
        let ts = self.last_event_unix_ms.load(Ordering::Relaxed);
        if ts == 0 {
            None
        } else {
            Some(ts)
        }
    }

    /// Start watching the given roots.
    ///
    /// # Errors
    /// Returns an error if:
    /// - The watcher is already running
    /// - A root path is invalid
    /// - The watcher cannot be created
    pub fn start(&self, roots: Vec<String>) -> Result<(), WatchError> {
        // Check if already running
        if self.running.load(Ordering::Relaxed) {
            return Err(WatchError::AlreadyRunning);
        }

        // Validate roots
        let mut validated_roots = Vec::new();
        for root in &roots {
            let path = PathBuf::from(root);
            if !path.exists() {
                return Err(WatchError::InvalidRoot(root.clone()));
            }
            if !path.is_dir() {
                return Err(WatchError::InvalidRoot(root.clone()));
            }
            validated_roots.push(path);
        }

        // Create event channel
        let (tx, mut rx) = mpsc::unbounded_channel::<WatchEvent>();

        // Create the watcher
        let tx_clone = tx.clone();

        let watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                match res {
                    Ok(event) => {
                        // Filter events we care about
                        if should_process_event(&event) {
                            let watch_event = WatchEvent {
                                paths: event.paths.clone(),
                                kind: WatchEventKind::from(&event.kind),
                            };

                            if let Err(e) = tx_clone.send(watch_event) {
                                warn!(error = %e, "Failed to send watch event");
                            }
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Watch error");
                    }
                }
            },
            Config::default().with_poll_interval(Duration::from_secs(2)),
        )
        .map_err(|e| WatchError::WatcherFailed(e.to_string()))?;

        // Watch each root
        let mut watcher = watcher;
        for root in &validated_roots {
            watcher
                .watch(root, RecursiveMode::Recursive)
                .map_err(|e| WatchError::WatcherFailed(e.to_string()))?;
            info!(root = %root.display(), "Watching directory");
        }

        // Store state
        *self.roots.write().unwrap() = roots;
        *self.watcher.lock().unwrap() = Some(watcher);
        *self.event_tx.lock().unwrap() = Some(tx);
        self.running.store(true, Ordering::Relaxed);

        // Get cache references for event processor
        let cache = self.cache.lock().unwrap().clone();
        let pkg_json_cache = self.pkg_json_cache.lock().unwrap().clone();
        let build_cache = self.build_cache.lock().unwrap().clone();
        let last_event_store = self.last_event_unix_ms.clone();
        let build_watchers = self.build_watchers.clone();

        // Spawn event processor
        tokio::spawn(async move {
            process_events(
                &mut rx,
                cache.as_ref(),
                pkg_json_cache.as_ref(),
                build_cache.as_ref(),
                &last_event_store,
                &build_watchers,
            )
            .await;
        });

        Ok(())
    }

    /// Stop watching.
    ///
    /// # Errors
    /// Returns an error if the watcher is not running.
    pub fn stop(&self) -> Result<(), WatchError> {
        if !self.running.load(Ordering::Relaxed) {
            return Err(WatchError::NotRunning);
        }

        // Drop the watcher
        *self.watcher.lock().unwrap() = None;
        *self.event_tx.lock().unwrap() = None;

        // Clear state
        self.roots.write().unwrap().clear();
        self.running.store(false, Ordering::Relaxed);

        info!("File watcher stopped");

        Ok(())
    }

    /// Watch a directory for build mode (v3.0).
    /// Notifications are sent to the provided channel when files change.
    ///
    /// # Errors
    /// Returns an error if the path is invalid or watcher cannot be set up.
    pub fn watch_for_build(&self, path: &PathBuf, tx: mpsc::Sender<()>) -> Result<(), WatchError> {
        // Validate path
        if !path.exists() || !path.is_dir() {
            return Err(WatchError::InvalidRoot(path.display().to_string()));
        }

        // Add subscriber
        {
            let mut watchers = self.build_watchers.lock().unwrap();
            watchers.push((path.clone(), tx));
        }

        // If watcher not running, start it for this path
        if !self.running.load(Ordering::Relaxed) {
            self.start(vec![path.display().to_string()])?;
        } else {
            // Add path to existing watcher if not already watching
            let mut roots = self.roots.write().unwrap();
            let path_str = path.display().to_string();
            if !roots.contains(&path_str) {
                if let Some(watcher) = self.watcher.lock().unwrap().as_mut() {
                    watcher
                        .watch(path, RecursiveMode::Recursive)
                        .map_err(|e| WatchError::WatcherFailed(e.to_string()))?;
                    roots.push(path_str);
                    info!(root = %path.display(), "Added directory to watcher");
                }
            }
        }

        Ok(())
    }

    /// Stop watching a specific directory (v3.0).
    pub fn unwatch(&self, path: &PathBuf) {
        // Remove subscriber
        {
            let mut watchers = self.build_watchers.lock().unwrap();
            watchers.retain(|(p, _)| p != path);
        }

        // Optionally unwatch from file system if no other subscribers for this path
        let has_other_subscribers = {
            let watchers = self.build_watchers.lock().unwrap();
            watchers.iter().any(|(p, _)| p == path)
        };

        if !has_other_subscribers {
            if let Some(watcher) = self.watcher.lock().unwrap().as_mut() {
                let _ = watcher.unwatch(path);
                info!(root = %path.display(), "Removed directory from watcher");
            }
        }
    }

}

/// Process events with coalescing.
async fn process_events(
    rx: &mut mpsc::UnboundedReceiver<WatchEvent>,
    cache: Option<&Arc<DaemonResolverCache>>,
    pkg_json_cache: Option<&Arc<DaemonPkgJsonCache>>,
    build_cache: Option<&Arc<DaemonBuildCache>>,
    last_event_store: &Arc<AtomicU64>,
    build_watchers: &Arc<Mutex<Vec<(PathBuf, mpsc::Sender<()>)>>>,
) {
    let mut pending_paths: HashSet<PathBuf> = HashSet::new();
    let mut last_event_time = std::time::Instant::now();

    loop {
        let timeout =
            tokio::time::timeout(Duration::from_millis(COALESCE_WINDOW_MS), rx.recv()).await;

        match timeout {
            Ok(Some(event)) => {
                // Accumulate paths
                for path in event.paths {
                    pending_paths.insert(path);
                }
                last_event_time = std::time::Instant::now();
            }
            Ok(None) => {
                // Channel closed
                debug!("Watch event channel closed");
                break;
            }
            Err(_) => {
                // Timeout - process pending if we have any and enough time has passed
                if !pending_paths.is_empty()
                    && last_event_time.elapsed() >= Duration::from_millis(COALESCE_WINDOW_MS)
                {
                    // Process coalesced events
                    debug!(
                        count = pending_paths.len(),
                        "Processing coalesced file events"
                    );

                    let mut total_invalidated = 0;
                    let mut pkg_json_invalidated = 0;
                    let mut build_invalidated = 0;

                    for path in &pending_paths {
                        debug!(path = %path.display(), "File changed");

                        // Invalidate resolver cache entries for this path
                        if let Some(cache) = cache {
                            let count = cache.invalidate_path(path);
                            total_invalidated += count;
                        }

                        // Invalidate package.json cache if this is a package.json file
                        if let Some(pkg_cache) = pkg_json_cache {
                            if is_package_json(path) && pkg_cache.invalidate(path) {
                                pkg_json_invalidated += 1;
                            }
                        }

                        // Invalidate build cache entries for this path
                        if let Some(build_cache) = build_cache {
                            let count = build_cache.invalidate_path(path);
                            build_invalidated += count;
                        }
                    }

                    if total_invalidated > 0 {
                        debug!(
                            count = total_invalidated,
                            "Resolver cache entries invalidated"
                        );
                    }
                    if pkg_json_invalidated > 0 {
                        debug!(
                            count = pkg_json_invalidated,
                            "Package.json cache entries invalidated"
                        );
                    }
                    if build_invalidated > 0 {
                        debug!(
                            count = build_invalidated,
                            "Build cache entries invalidated"
                        );
                    }

                    // Update timestamp AFTER invalidation is applied
                    #[allow(clippy::cast_possible_truncation)]
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);
                    last_event_store.store(now, Ordering::Relaxed);

                    // Notify build watchers (v3.0)
                    {
                        let watchers = build_watchers.lock().unwrap();
                        for (watch_path, tx) in watchers.iter() {
                            // Check if any changed path is under this watch path
                            for changed in &pending_paths {
                                if changed.starts_with(watch_path) {
                                    // Send notification (non-blocking)
                                    let _ = tx.try_send(());
                                    break; // Only need to notify once per watcher
                                }
                            }
                        }
                    }

                    pending_paths.clear();
                }
            }
        }
    }
}

/// Check if a path is a package.json file.
fn is_package_json(path: &std::path::Path) -> bool {
    path.file_name()
        .map(|n| n == "package.json")
        .unwrap_or(false)
}

/// Check if we should process this event.
fn should_process_event(event: &Event) -> bool {
    match &event.kind {
        // File creation
        EventKind::Create(CreateKind::File) => true,
        // File modification
        EventKind::Modify(ModifyKind::Data(_)) => true,
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => true,
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) => true,
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => true,
        // File removal
        EventKind::Remove(RemoveKind::File) => true,
        // Ignore directories, metadata changes, and other events
        _ => false,
    }
}

/// Watcher error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchError {
    AlreadyRunning,
    NotRunning,
    InvalidRoot(String),
    WatcherFailed(String),
}

impl std::fmt::Display for WatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyRunning => write!(f, "Watcher is already running"),
            Self::NotRunning => write!(f, "Watcher is not running"),
            Self::InvalidRoot(root) => write!(f, "Invalid watch root: {root}"),
            Self::WatcherFailed(msg) => write!(f, "Watcher failed: {msg}"),
        }
    }
}

impl std::error::Error for WatchError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watcher_state_new() {
        let state = WatcherState::new();
        assert!(!state.is_running());
        assert!(state.roots().is_empty());
        assert!(state.last_event_unix_ms().is_none());
    }

    #[test]
    fn test_watcher_not_running_stop_fails() {
        let state = WatcherState::new();
        let result = state.stop();
        assert!(matches!(result, Err(WatchError::NotRunning)));
    }

    #[test]
    fn test_watch_event_kind_from_notify() {
        assert_eq!(
            WatchEventKind::from(&EventKind::Create(CreateKind::File)),
            WatchEventKind::Create
        );
        assert_eq!(
            WatchEventKind::from(&EventKind::Remove(RemoveKind::File)),
            WatchEventKind::Remove
        );
    }
}
