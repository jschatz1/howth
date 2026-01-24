#![deny(clippy::all)]
#![warn(clippy::pedantic)]

//! Node API compatibility layer for fastnode.
//!
//! This crate will provide compatibility with Node.js APIs, allowing fastnode
//! to run Node.js programs and npm packages.
//!
//! Currently a placeholder - implementation coming in future milestones.

/// Placeholder module for future Node.js fs API compatibility.
#[cfg(any(feature = "engine-v8", feature = "engine-sm", feature = "engine-jsc"))]
pub mod fs {
    /// Placeholder for Node fs compatibility.
    #[must_use]
    pub fn placeholder() -> &'static str {
        "fastnode-compat: Node fs API not yet implemented"
    }
}

/// Placeholder module for future Node.js path API compatibility.
#[cfg(any(feature = "engine-v8", feature = "engine-sm", feature = "engine-jsc"))]
pub mod path {
    /// Placeholder for Node path compatibility.
    #[must_use]
    pub fn placeholder() -> &'static str {
        "fastnode-compat: Node path API not yet implemented"
    }
}

/// Stub module when no engine is enabled.
#[cfg(not(any(feature = "engine-v8", feature = "engine-sm", feature = "engine-jsc")))]
pub mod stub {
    /// Returns info about the stub state.
    #[must_use]
    pub fn info() -> &'static str {
        "fastnode-compat: No JS engine enabled. Enable engine-v8, engine-sm, or engine-jsc feature."
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_stub_exists() {
        #[cfg(not(any(feature = "engine-v8", feature = "engine-sm", feature = "engine-jsc")))]
        {
            let info = super::stub::info();
            assert!(!info.is_empty());
        }
    }
}
