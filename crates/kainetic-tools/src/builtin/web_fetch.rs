//! [`WebFetchTool`] — fetches a URL and returns its content as plain text.

use kainetic_schema::RootSchema;
use schemars::schema_for;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::{Tool, ToolContext, ToolError, ToolFuture};

/// Fetches a URL and returns the response body as plain text.
///
/// HTML pages are stripped to their text content via
/// [`scraper`] before returning, reducing noise in LLM context.
///
/// # Tool input schema
///
/// ```json
/// {
///   "url": "https://example.com",
///   "extract_text": true
/// }
/// ```
pub struct WebFetchTool {
    client: reqwest::Client,
}

impl WebFetchTool {
    /// Creates a new `WebFetchTool` with a default HTTP client.
    ///
    /// # Panics
    ///
    /// Panics if the TLS backend fails to initialise (extremely unlikely on
    /// supported platforms).
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("kainetic-agent/0.1")
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("reqwest client build failed"),
        }
    }
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Deserialize, JsonSchema)]
struct Input {
    url: String,
    #[serde(default = "default_true")]
    extract_text: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Serialize, JsonSchema)]
struct Output {
    url: String,
    status_code: u16,
    content: String,
    content_type: String,
}

impl Tool for WebFetchTool {
    fn name(&self) -> &'static str {
        "web_fetch"
    }

    fn description(&self) -> &'static str {
        "Fetch a URL and return its content. HTML pages are automatically converted to plain text."
    }

    fn input_schema(&self) -> RootSchema {
        schema_for!(Input)
    }

    fn output_schema(&self) -> RootSchema {
        schema_for!(Output)
    }

    fn call(&self, input: serde_json::Value, ctx: ToolContext) -> ToolFuture<'_> {
        Box::pin(async move {
            let params: Input = serde_json::from_value(input)
                .map_err(|e| ToolError::InputValidation(e.to_string()))?;

            debug!(url = %params.url, "web_fetch: fetching");

            let fetch = self.client.get(&params.url).send();

            let response = tokio::select! {
                res = fetch => res.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?,
                () = ctx.cancellation_token.cancelled() => {
                    return Err(ToolError::Cancelled);
                }
            };

            let status_code = response.status().as_u16();
            let content_type = response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("text/plain")
                .to_owned();

            let raw_body = response
                .text()
                .await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

            let content = if params.extract_text && content_type.contains("text/html") {
                html_to_text(&raw_body)
            } else {
                raw_body
            };

            serde_json::to_value(Output {
                url: params.url,
                status_code,
                content,
                content_type,
            })
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
        })
    }
}

/// Strips HTML tags and returns only visible text, preserving block structure.
fn html_to_text(html: &str) -> String {
    use scraper::{Html, Selector};

    let document = Html::parse_document(html);
    // Select all text-bearing block elements.
    let selector =
        Selector::parse("p, h1, h2, h3, h4, h5, h6, li, td, th, div, span, a, pre, code")
            .expect("valid selector");

    let mut parts = Vec::new();
    for element in document.select(&selector) {
        let text = element
            .text()
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if !text.is_empty() {
            parts.push(text);
        }
    }

    // Deduplicate consecutive identical lines.
    parts.dedup();
    parts.join("\n")
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_util::sync::CancellationToken;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::context::ToolContext;
    use kainetic_schema::RunId;

    fn ctx() -> ToolContext {
        ToolContext::new(RunId::new(), CancellationToken::new())
    }

    #[tokio::test]
    async fn fetch_plain_text() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/hello"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/plain")
                    .set_body_string("hello world"),
            )
            .mount(&server)
            .await;

        let tool = WebFetchTool::new();
        let result = tool
            .call(
                serde_json::json!({ "url": format!("{}/hello", server.uri()) }),
                ctx(),
            )
            .await
            .unwrap();

        assert_eq!(result["status_code"], 200);
        assert_eq!(result["content"], "hello world");
    }

    #[tokio::test]
    async fn fetch_html_extracts_text() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/page"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html")
                    .set_body_string("<html><body><h1>Title</h1><p>Body text.</p></body></html>"),
            )
            .mount(&server)
            .await;

        let tool = WebFetchTool::new();
        let result = tool
            .call(
                serde_json::json!({ "url": format!("{}/page", server.uri()) }),
                ctx(),
            )
            .await
            .unwrap();

        let content = result["content"].as_str().unwrap();
        assert!(content.contains("Title"));
        assert!(content.contains("Body text"));
    }

    #[test]
    fn html_to_text_basic() {
        let html = "<html><body><h1>Hello</h1><p>World</p></body></html>";
        let text = html_to_text(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
    }

    #[test]
    fn input_schema_valid() {
        let tool = WebFetchTool::new();
        let schema = tool.input_schema();
        assert!(schema.schema.metadata.is_some());
    }
}
