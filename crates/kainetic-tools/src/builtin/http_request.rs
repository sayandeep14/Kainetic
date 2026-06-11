//! `HttpRequestTool` — generic HTTP client tool.

use std::collections::HashMap;

use kainetic_schema::RootSchema;
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};

use crate::{Tool, ToolContext, ToolError, ToolFuture};

// ─── Input / output types ─────────────────────────────────────────────────────

/// Input for [`HttpRequestTool`].
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct HttpRequestInput {
    /// HTTP method in uppercase: `GET`, `POST`, `PUT`, `DELETE`, `PATCH`, etc.
    pub method: String,
    /// Fully-qualified target URL including scheme (e.g. `"https://example.com/api"`).
    pub url: String,
    /// Optional request headers (key → value).
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Optional request body.
    ///
    /// When present, the `Content-Type` header is set to `application/json` if
    /// not already provided.
    pub body: Option<serde_json::Value>,
    /// Request timeout in seconds. Defaults to 30.
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_timeout_secs() -> u64 {
    30
}

/// Output of [`HttpRequestTool`].
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct HttpRequestOutput {
    /// HTTP status code returned by the server.
    pub status: u16,
    /// Response headers (key → value; multi-value headers are joined with `, `).
    pub headers: HashMap<String, String>,
    /// Response body decoded as UTF-8 text.
    pub body: String,
}

// ─── Tool ─────────────────────────────────────────────────────────────────────

/// Tool that makes an HTTP request and returns the status, headers, and body.
///
/// Uses a shared [`reqwest::Client`] for connection pooling. Construct with
/// [`HttpRequestTool::new`] or supply a pre-configured client via
/// [`HttpRequestTool::with_client`].
pub struct HttpRequestTool {
    client: reqwest::Client,
}

impl HttpRequestTool {
    /// Creates a new tool with a default `reqwest` client.
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Creates a new tool that reuses the provided `reqwest` client.
    #[must_use]
    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl Default for HttpRequestTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for HttpRequestTool {
    fn name(&self) -> &'static str {
        "http_request"
    }

    fn description(&self) -> &'static str {
        "Makes an HTTP request to a URL and returns the status code, headers, and body."
    }

    fn input_schema(&self) -> RootSchema {
        schema_for!(HttpRequestInput)
    }

    fn output_schema(&self) -> RootSchema {
        schema_for!(HttpRequestOutput)
    }

    fn call(&self, input: serde_json::Value, ctx: ToolContext) -> ToolFuture<'_> {
        let client = self.client.clone();
        Box::pin(async move {
            if ctx.cancellation_token.is_cancelled() {
                return Err(ToolError::Cancelled);
            }

            let req: HttpRequestInput = serde_json::from_value(input)
                .map_err(|e| ToolError::InputValidation(e.to_string()))?;

            let method = reqwest::Method::from_bytes(req.method.to_uppercase().as_bytes())
                .map_err(|_| {
                    ToolError::ExecutionFailed(format!("invalid HTTP method: {}", req.method))
                })?;

            let timeout = std::time::Duration::from_secs(req.timeout_secs);
            let mut builder = client.request(method, &req.url).timeout(timeout);

            for (key, value) in &req.headers {
                builder = builder.header(key.as_str(), value.as_str());
            }

            if let Some(body) = req.body {
                builder = builder.json(&body);
            }

            let response = builder
                .send()
                .await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

            let status = response.status().as_u16();
            let response_headers: HashMap<String, String> = response
                .headers()
                .iter()
                .filter_map(|(k, v)| v.to_str().ok().map(|s| (k.to_string(), s.to_owned())))
                .collect();
            let body = response
                .text()
                .await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

            serde_json::to_value(HttpRequestOutput {
                status,
                headers: response_headers,
                body,
            })
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
        })
    }
}

#[cfg(test)]
mod tests {
    use kainetic_schema::RunId;
    use tokio_util::sync::CancellationToken;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::{Tool, ToolContext};

    fn ctx() -> ToolContext {
        ToolContext::new(RunId::new(), CancellationToken::new())
    }

    #[test]
    fn name_and_description() {
        let tool = HttpRequestTool::new();
        assert_eq!(tool.name(), "http_request");
        assert!(!tool.description().is_empty());
    }

    #[tokio::test]
    async fn get_request_returns_body() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/hello"))
            .respond_with(ResponseTemplate::new(200).set_body_string("world"))
            .mount(&server)
            .await;

        let tool = HttpRequestTool::new();
        let result = tool
            .call(
                serde_json::json!({
                    "method": "GET",
                    "url": format!("{}/hello", server.uri())
                }),
                ctx(),
            )
            .await
            .unwrap();

        assert_eq!(result["status"], 200);
        assert_eq!(result["body"], "world");
    }

    #[tokio::test]
    async fn post_request_with_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/data"))
            .respond_with(ResponseTemplate::new(201).set_body_string("created"))
            .mount(&server)
            .await;

        let tool = HttpRequestTool::new();
        let result = tool
            .call(
                serde_json::json!({
                    "method": "POST",
                    "url": format!("{}/data", server.uri()),
                    "body": {"key": "value"}
                }),
                ctx(),
            )
            .await
            .unwrap();

        assert_eq!(result["status"], 201);
    }

    #[tokio::test]
    async fn non_200_status_is_returned_not_errored() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
            .mount(&server)
            .await;

        let tool = HttpRequestTool::new();
        let result = tool
            .call(
                serde_json::json!({
                    "method": "GET",
                    "url": format!("{}/missing", server.uri())
                }),
                ctx(),
            )
            .await
            .unwrap();

        assert_eq!(result["status"], 404);
        assert_eq!(result["body"], "not found");
    }

    #[tokio::test]
    async fn cancelled_before_call_returns_error() {
        let token = CancellationToken::new();
        token.cancel();
        let ctx = ToolContext::new(RunId::new(), token);

        let tool = HttpRequestTool::new();
        let err = tool
            .call(
                serde_json::json!({"method": "GET", "url": "http://example.com"}),
                ctx,
            )
            .await
            .unwrap_err();

        assert!(matches!(err, ToolError::Cancelled));
    }
}
