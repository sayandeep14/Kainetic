//! The production-grade Rust runtime for agentic AI.
//!
//! This is the facade crate. It re-exports the public API of all Kainetic
//! sub-crates so users can add a single `kainetic` dependency and access
//! everything they need.
//!
//! # Quick Start
//!
//! ```text
//! // Coming in Part 4 — once KaineticRuntime is implemented.
//! ```
#![deny(clippy::all, clippy::pedantic, missing_docs, unsafe_code)]

pub use kainetic_core as core;
pub use kainetic_memory as memory;
pub use kainetic_orchestra as orchestra;
pub use kainetic_providers as providers;
pub use kainetic_schema as schema;
pub use kainetic_telemetry as telemetry;
pub use kainetic_tools as tools;
