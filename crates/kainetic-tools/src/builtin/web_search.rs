//! `WebSearchTool` — web search via the Brave Search API.

use kainetic_schema::RootSchema;
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};

use crate::{Tool, ToolContext, ToolError, ToolFuture};

// ─── Input / output types ─────────────────────────────────────────────────────

/// Input for [`WebSearchTool`].
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct WebSearchInput {
    /// The search query string.
    pub query: String,
    /// Maximum number of results to return (1–10). Defaults to 5.
    #[serde(default = "default_count")]
    pub count: u32,
}

fn default_count() -> u32 {
    5
}

/// Output of [`WebSearchTool`].
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct WebSearchOutput {
    /// Ranked list of search results.
    pub results: Vec<SearchResult>,
}

/// A single web search result.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SearchResult {
    /// Page title.
    pub title: String,
    /// Canonical URL of the result.
    pub url: String,
    /// Short text excerpt from the page.
    pub snippet: String,
}

// ─── Brave API wire types ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct BraveResponse {
    web: Option<BraveWebResults>,
}

#[derive(Deserialize)]
struct BraveWebResults {
    results: Vec<BraveResult>,
}

#[derive(Deserialize)]
struct BraveResult {
    title: String,
    url: String,
    #[serde(default)]
    description: String,
}

// ─── Tool ─────────────────────────────────────────────────────────────────────

/// Tool that performs a web search using the Brave Search API and returns
/// a ranked list of results.
///
/// # Authentication
///
/// Set the `BRAVE_SEARCH_API_KEY` environment variable, or pass the key
/// directly to [`WebSearchTool::new`].
pub struct WebSearchTool {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl WebSearchTool {
    /// Creates a tool using an explicit Brave Search API key.
    #[must_use]
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            base_url: "https://api.search.brave.com".to_owned(),
        }
    }

    /// Creates a tool by reading `BRAVE_SEARCH_API_KEY` from the environment.
    ///
    /// # Errors
    ///
    /// Returns [`ToolError::ExecutionFailed`] if the environment variable is
    /// not set.
    pub fn from_env() -> Result<Self, ToolError> {
        let api_key = std::env::var("BRAVE_SEARCH_API_KEY").map_err(|_| {
            ToolError::ExecutionFailed(
                "BRAVE_SEARCH_API_KEY environment variable is not set".to_owned(),
            )
        })?;
        Ok(Self::new(api_key))
    }

    /// Creates a tool with a custom base URL (for testing with mock servers).
    #[must_use]
    pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            base_url: base_url.into(),
        }
    }
}

impl Tool for WebSearchTool {
    fn name(&self) -> &'static str {
        "web_search"
    }

    fn description(&self) -> &'static str {
        "Searches the web for current information and returns a ranked list of results."
    }

    fn input_schema(&self) -> RootSchema {
        schema_for!(WebSearchInput)
    }

    fn output_schema(&self) -> RootSchema {
        schema_for!(WebSearchOutput)
    }

    fn call(&self, input: serde_json::Value, ctx: ToolContext) -> ToolFuture<'_> {
        let client = self.client.clone();
        let api_key = self.api_key.clone();
        let base_url = self.base_url.clone();

        Box::pin(async move {
            if ctx.cancellation_token.is_cancelled() {
                return Err(ToolError::Cancelled);
            }

            let req: WebSearchInput = serde_json::from_value(input)
                .map_err(|e| ToolError::InputValidation(e.to_string()))?;

            let count = req.count.clamp(1, 10);
            let response = client
                .get(format!("{base_url}/res/v1/web/search"))
                .header("Accept", "application/json")
                .header("X-Subscription-Token", &api_key)
                .query(&[("q", &req.query), ("count", &count.to_string())])
                .timeout(std::time::Duration::from_secs(15))
                .send()
                .await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

            if !response.status().is_success() {
                let status = response.status().as_u16();
                let body = response.text().await.unwrap_or_default();
                return Err(ToolError::ExecutionFailed(format!(
                    "Brave Search API returned {status}: {body}"
                )));
            }

            let brave: BraveResponse = response
                .json()
                .await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

            let results = brave
                .web
                .map(|w| w.results)
                .unwrap_or_default()
                .into_iter()
                .map(|r| SearchResult {
                    title: r.title,
                    url: r.url,
                    snippet: r.description,
                })
                .collect();

            serde_json::to_value(WebSearchOutput { results })
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
        })
    }
}

#[cfg(test)]
mod tests {
    use kainetic_schema::RunId;
    use tokio_util::sync::CancellationToken;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::{Tool, ToolContext};

    fn ctx() -> ToolContext {
        ToolContext::new(RunId::new(), CancellationToken::new())
    }

    fn brave_response_body() -> serde_json::Value {
        serde_json::json!({
            "web": {
                "results": [
                    {
                        "title": "Rust Programming Language",
                        "url": "https://www.rust-lang.org",
                        "description": "A language empowering everyone to build reliable software."
                    },
                    {
                        "title": "Rust Book",
                        "url": "https://doc.rust-lang.org/book/",
                        "description": "The official Rust programming language book."
                    }
                ]
            }
        })
    }

    #[test]
    fn name_and_description() {
        let tool = WebSearchTool::new("test-key");
        assert_eq!(tool.name(), "web_search");
        assert!(!tool.description().is_empty());
    }

    #[tokio::test]
    async fn search_returns_parsed_results() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/res/v1/web/search"))
            .and(header("X-Subscription-Token", "test-key"))
            .and(query_param("q", "rust programming"))
            .respond_with(ResponseTemplate::new(200).set_body_json(brave_response_body()))
            .mount(&server)
            .await;

        let tool = WebSearchTool::with_base_url("test-key", server.uri());
        let result = tool
            .call(serde_json::json!({"query": "rust programming"}), ctx())
            .await
            .unwrap();

        let results = result["results"].as_array().unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["title"], "Rust Programming Language");
        assert_eq!(results[0]["url"], "https://www.rust-lang.org");
    }

    #[tokio::test]
    async fn search_handles_empty_results() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/res/v1/web/search"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"web": null})),
            )
            .mount(&server)
            .await;

        let tool = WebSearchTool::with_base_url("key", server.uri());
        let result = tool
            .call(serde_json::json!({"query": "xyzzy"}), ctx())
            .await
            .unwrap();

        assert_eq!(result["results"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn search_propagates_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(429).set_body_string("rate limited"))
            .mount(&server)
            .await;

        let tool = WebSearchTool::with_base_url("key", server.uri());
        let err = tool
            .call(serde_json::json!({"query": "rust"}), ctx())
            .await
            .unwrap_err();

        assert!(matches!(err, ToolError::ExecutionFailed(_)));
    }

    #[tokio::test]
    async fn cancelled_before_call_returns_error() {
        let token = CancellationToken::new();
        token.cancel();
        let ctx = ToolContext::new(RunId::new(), token);

        let tool = WebSearchTool::new("key");
        let err = tool
            .call(serde_json::json!({"query": "rust"}), ctx)
            .await
            .unwrap_err();

        assert!(matches!(err, ToolError::Cancelled));
    }

    #[cfg(feature = "integration")]
    #[tokio::test]
    async fn integration_search() {
        let tool = WebSearchTool::from_env().expect("BRAVE_SEARCH_API_KEY must be set");
        let result = tool
            .call(
                serde_json::json!({"query": "Rust programming language", "count": 3}),
                ctx(),
            )
            .await
            .expect("search must succeed");

        let results = result["results"].as_array().unwrap();
        assert!(!results.is_empty(), "expected at least one result");
    }
}
