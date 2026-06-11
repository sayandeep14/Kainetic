//! Built-in tools shipped with Kainetic.
//!
//! These implement [`crate::Tool`] manually, demonstrating the pattern that
//! the `#[tool]` macro automates for user-defined tools.

pub mod datetime;
pub mod http_request;
pub mod web_search;

pub use datetime::CurrentDatetimeTool;
pub use http_request::HttpRequestTool;
pub use web_search::WebSearchTool;
