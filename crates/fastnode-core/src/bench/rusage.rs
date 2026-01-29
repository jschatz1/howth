//! Resource usage measurement via `getrusage(2)`.
//!
//! Provides CPU time (user + system) and peak RSS tracking for benchmarks.
//! On non-Unix platforms, all functions return `None`.

/// A snapshot of resource usage from `getrusage`.
#[derive(Debug, Clone, Copy)]
pub struct RusageSnapshot {
    /// User CPU time in microseconds.
    pub user_time_us: u64,
    /// System CPU time in microseconds.
    pub system_time_us: u64,
    /// Maximum resident set size in bytes.
    pub max_rss: u64,
}

/// Delta between two resource usage snapshots.
#[derive(Debug, Clone, Copy)]
pub struct RusageDelta {
    /// User CPU time delta in microseconds.
    pub user_time_us: u64,
    /// System CPU time delta in microseconds.
    pub system_time_us: u64,
    /// Maximum resident set size in bytes (from the "after" snapshot).
    pub max_rss: u64,
}

impl RusageDelta {
    /// Total CPU time (user + system) in microseconds.
    pub fn total_cpu_us(&self) -> u64 {
        self.user_time_us + self.system_time_us
    }
}

/// Compute the delta between two snapshots.
pub fn delta(before: &RusageSnapshot, after: &RusageSnapshot) -> RusageDelta {
    RusageDelta {
        user_time_us: after.user_time_us.saturating_sub(before.user_time_us),
        system_time_us: after.system_time_us.saturating_sub(before.system_time_us),
        max_rss: after.max_rss,
    }
}

#[cfg(unix)]
mod platform {
    use super::RusageSnapshot;

    /// Convert a `libc::rusage` to a `RusageSnapshot`.
    #[allow(clippy::cast_sign_loss)]
    fn from_rusage(ru: libc::rusage) -> RusageSnapshot {
        let user_time_us = ru.ru_utime.tv_sec as u64 * 1_000_000 + ru.ru_utime.tv_usec as u64;
        let system_time_us =
            ru.ru_stime.tv_sec as u64 * 1_000_000 + ru.ru_stime.tv_usec as u64;

        // On macOS, ru_maxrss is in bytes. On Linux, it's in kilobytes.
        let max_rss = if cfg!(target_os = "macos") {
            ru.ru_maxrss as u64
        } else {
            ru.ru_maxrss as u64 * 1024
        };

        RusageSnapshot {
            user_time_us,
            system_time_us,
            max_rss,
        }
    }

    /// Snapshot resource usage for the current process (`RUSAGE_SELF`).
    pub fn snapshot_self() -> Option<RusageSnapshot> {
        let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
        let ret = unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) };
        if ret == 0 {
            Some(from_rusage(ru))
        } else {
            None
        }
    }

    /// Snapshot resource usage for child processes (`RUSAGE_CHILDREN`).
    pub fn snapshot_children() -> Option<RusageSnapshot> {
        let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
        let ret = unsafe { libc::getrusage(libc::RUSAGE_CHILDREN, &mut ru) };
        if ret == 0 {
            Some(from_rusage(ru))
        } else {
            None
        }
    }
}

#[cfg(not(unix))]
mod platform {
    use super::RusageSnapshot;

    pub fn snapshot_self() -> Option<RusageSnapshot> {
        None
    }

    pub fn snapshot_children() -> Option<RusageSnapshot> {
        None
    }
}

/// Snapshot resource usage for the current process.
pub fn snapshot_self() -> Option<RusageSnapshot> {
    platform::snapshot_self()
}

/// Snapshot resource usage for child processes.
pub fn snapshot_children() -> Option<RusageSnapshot> {
    platform::snapshot_children()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn test_snapshot_self_returns_some() {
        let snap = snapshot_self();
        assert!(snap.is_some());
    }

    #[test]
    #[cfg(unix)]
    fn test_snapshot_children_returns_some() {
        let snap = snapshot_children();
        assert!(snap.is_some());
    }

    #[test]
    fn test_delta_computation() {
        let before = RusageSnapshot {
            user_time_us: 100,
            system_time_us: 50,
            max_rss: 1024,
        };
        let after = RusageSnapshot {
            user_time_us: 300,
            system_time_us: 150,
            max_rss: 2048,
        };
        let d = delta(&before, &after);
        assert_eq!(d.user_time_us, 200);
        assert_eq!(d.system_time_us, 100);
        assert_eq!(d.total_cpu_us(), 300);
        assert_eq!(d.max_rss, 2048);
    }

    #[test]
    fn test_delta_saturating_sub() {
        let before = RusageSnapshot {
            user_time_us: 500,
            system_time_us: 200,
            max_rss: 1024,
        };
        let after = RusageSnapshot {
            user_time_us: 100,
            system_time_us: 50,
            max_rss: 512,
        };
        let d = delta(&before, &after);
        assert_eq!(d.user_time_us, 0);
        assert_eq!(d.system_time_us, 0);
    }
}
