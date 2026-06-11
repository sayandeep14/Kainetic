//! Tool trait, registry, input validation, and built-in tools for Kainetic.
//!
//! Provides the [`Tool`] trait that all tools implement, a lock-free
//! [`ToolRegistry`] backed by `DashMap`, JSON Schema-based input validation
//! that runs before every tool call, and built-in tools such as
//! [`builtin::CurrentDatetimeTool`], [`builtin::HttpRequestTool`], and
//! [`builtin::WebSearchTool`].
//!
//! # Implementing a custom tool
//!
//! The simplest way is the `#[tool]` proc macro from `kainetic-macros`:
//!
//! ```rust,no_run
//! use kainetic_tools::{ToolContext, ToolError};
//! use serde::{Deserialize, Serialize};
//! use schemars::JsonSchema;
//!
//! #[derive(Deserialize, JsonSchema)]
//! pub struct AddInput { pub a: f64, pub b: f64 }
//!
//! #[derive(Serialize, JsonSchema)]
//! pub struct AddOutput { pub sum: f64 }
//!
//! # // doc-test compiled only; macro not invoked in doc-test context
//! // #[kainetic_macros::tool(description = "Adds two numbers.")]
//! // async fn add(input: AddInput, _ctx: ToolContext) -> Result<AddOutput, ToolError> {
//! //     Ok(AddOutput { sum: input.a + input.b })
//! // }
//! // Then: registry.register(Add);
//! ```
//!
//! Or implement [`Tool`] manually for full control.
#![deny(clippy::all, clippy::pedantic, missing_docs, unsafe_code)]

pub mod builtin;
mod context;
mod error;
mod registry;
mod tool;

pub use context::ToolContext;
pub use error::ToolError;
pub use registry::ToolRegistry;
pub use tool::{Tool, ToolFuture};
