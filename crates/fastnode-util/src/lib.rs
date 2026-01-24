#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

//! Shared utilities for fastnode.
//!
//! This crate provides pure helper functions with no logging/tracing dependencies.
//! Logging is handled by the CLI crate to keep this library lightweight.

pub mod fs;
pub mod hash;
