//! Built-in tools shipped with Kainetic.
//!
//! These implement [`crate::Tool`] manually, demonstrating the pattern that
//! the `#[tool]` macro automates for user-defined tools.

pub mod code_executor;
pub mod datetime;
pub mod file_ops;
pub mod http_request;
pub mod shell;
pub mod sql_query;
pub mod vector_search;
pub mod web_fetch;
pub mod web_search;

pub use datetime::CurrentDatetimeTool;
pub use file_ops::{FileReadTool, FileWriteTool};
pub use http_request::HttpRequestTool;
pub use sql_query::SqlQueryTool;
pub use web_fetch::WebFetchTool;
pub use web_search::WebSearchTool;

#[cfg(feature = "code-executor")]
pub use code_executor::CodeExecutorTool;

#[cfg(feature = "shell")]
pub use shell::ShellTool;

#[cfg(feature = "vector-search")]
pub use vector_search::VectorSearchTool;
