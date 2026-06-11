//! Shared types, error enums, and JSON Schema infrastructure for Kainetic.
//!
//! This crate is the foundation every other Kainetic crate depends on.
//! It defines:
//!
//! - [`KaineticError`] — the top-level error type for the entire runtime
//! - [`RunId`], [`SessionId`], [`AgentId`], [`ToolId`] — typed identity newtypes
//! - [`TokenUsage`] and [`CostEstimate`] — token and cost tracking
//! - [`ToolDescriptor`] — static metadata every tool must expose
//! - [`Message`], [`MessageRole`], [`MessageContent`] — provider-agnostic conversation types
//! - [`JsonSchema`] re-export — derive JSON Schema from Rust types via `schemars`
//!
//! # Schema generation
//!
//! Re-export [`schemars::JsonSchema`] as [`JsonSchema`] so downstream crates
//! only need `kainetic-schema` in their dependency list:
//!
//! ```rust
//! use kainetic_schema::JsonSchema;
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Serialize, Deserialize, JsonSchema)]
//! struct MyInput {
//!     query: String,
//! }
//!
//! let schema = schemars::schema_for!(MyInput);
//! let value = serde_json::to_value(schema).unwrap();
//! assert_eq!(value["type"], "object");
//! ```
#![deny(clippy::all, clippy::pedantic, missing_docs, unsafe_code)]

mod error;
mod ids;
mod message;
mod token;
mod tool;

pub use error::KaineticError;
pub use ids::{AgentId, RunId, SessionId, ToolId};
pub use message::{Message, MessageContent, MessageRole};
pub use schemars::schema::RootSchema;
pub use schemars::JsonSchema;
pub use token::{CostEstimate, TokenUsage};
pub use tool::ToolDescriptor;
