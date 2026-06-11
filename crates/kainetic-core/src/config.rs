//! `AgentConfig` and `SystemPrompt` — runtime configuration for agents.

use std::{collections::HashMap, time::Duration};

/// A system prompt template with `{{variable}}` interpolation.
///
/// Variable slots use double-brace syntax (`{{name}}`). Calling [`render`]
/// replaces each slot with the corresponding value from the supplied map.
/// Unknown keys are left as-is.
///
/// # Example
///
/// ```
/// use std::collections::HashMap;
/// use kainetic_core::SystemPrompt;
///
/// let prompt = SystemPrompt::new("You are a {{role}}.");
/// let mut vars = HashMap::new();
/// vars.insert("role".to_owned(), "helpful assistant".to_owned());
/// assert_eq!(prompt.render(&vars), "You are a helpful assistant.");
/// ```
///
/// [`render`]: SystemPrompt::render
#[derive(Debug, Clone)]
pub struct SystemPrompt(String);

impl SystemPrompt {
    /// Creates a new prompt from a template string.
    #[must_use]
    pub fn new(template: impl Into<String>) -> Self {
        Self(template.into())
    }

    /// Renders the prompt by substituting `{{key}}` with the corresponding value.
    ///
    /// Keys not present in `vars` are left unchanged.
    #[must_use]
    pub fn render(&self, vars: &HashMap<String, String>) -> String {
        let mut out = self.0.clone();
        for (k, v) in vars {
            out = out.replace(&format!("{{{{{k}}}}}"), v);
        }
        out
    }

    /// Returns the raw template string without rendering.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for SystemPrompt {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for SystemPrompt {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

/// Runtime configuration for a single agent.
///
/// Build via [`AgentConfig::builder`] to get sensible defaults.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Model identifier to request (e.g. `"claude-sonnet-4-6"`).
    pub model: String,
    /// Optional system prompt prepended to every run.
    pub system_prompt: Option<SystemPrompt>,
    /// Maximum LLM → tool-calls → observe cycles before [`AgentError::MaxIterationsExceeded`].
    ///
    /// Default: `20`.
    ///
    /// [`AgentError::MaxIterationsExceeded`]: crate::AgentError::MaxIterationsExceeded
    pub max_iterations: u32,
    /// Maximum tokens to request per LLM call. Uses the provider default when `None`.
    pub max_tokens: Option<u32>,
    /// Sampling temperature for the LLM. `0.0` is deterministic, `1.0` is creative.
    pub temperature: Option<f32>,
    /// Whether to execute independent tool calls concurrently via `FuturesUnordered`.
    ///
    /// Default: `true`.
    pub parallel_tools: bool,
    /// Wall-clock timeout for the entire run. Default: `120` seconds.
    pub timeout: Option<Duration>,
}

impl AgentConfig {
    /// Returns a builder pre-filled with production-safe defaults.
    #[must_use]
    pub fn builder() -> AgentConfigBuilder {
        AgentConfigBuilder::default()
    }
}

/// Builder for [`AgentConfig`].
///
/// Obtain via [`AgentConfig::builder`].
#[derive(Debug, Clone)]
pub struct AgentConfigBuilder {
    model: String,
    system_prompt: Option<SystemPrompt>,
    max_iterations: u32,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    parallel_tools: bool,
    timeout: Option<Duration>,
}

impl Default for AgentConfigBuilder {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-6".to_owned(),
            system_prompt: None,
            max_iterations: 20,
            max_tokens: None,
            temperature: None,
            parallel_tools: true,
            timeout: None,
        }
    }
}

impl AgentConfigBuilder {
    /// Sets the model identifier.
    #[must_use]
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Sets the system prompt.
    #[must_use]
    pub fn system_prompt(mut self, prompt: impl Into<SystemPrompt>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Sets the maximum number of `ReAct` iterations.
    #[must_use]
    pub fn max_iterations(mut self, n: u32) -> Self {
        self.max_iterations = n;
        self
    }

    /// Sets the maximum tokens to request per LLM call.
    #[must_use]
    pub fn max_tokens(mut self, n: u32) -> Self {
        self.max_tokens = Some(n);
        self
    }

    /// Sets the sampling temperature.
    #[must_use]
    pub fn temperature(mut self, t: f32) -> Self {
        self.temperature = Some(t);
        self
    }

    /// Forces sequential tool execution (disables parallel dispatch).
    #[must_use]
    pub fn sequential_tools(mut self) -> Self {
        self.parallel_tools = false;
        self
    }

    /// Sets the wall-clock timeout for the entire run.
    #[must_use]
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    /// Builds the [`AgentConfig`].
    #[must_use]
    pub fn build(self) -> AgentConfig {
        AgentConfig {
            model: self.model,
            system_prompt: self.system_prompt,
            max_iterations: self.max_iterations,
            max_tokens: self.max_tokens,
            temperature: self.temperature,
            parallel_tools: self.parallel_tools,
            timeout: self.timeout,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_builder_has_expected_values() {
        let cfg = AgentConfig::builder().build();
        assert_eq!(cfg.model, "claude-sonnet-4-6");
        assert_eq!(cfg.max_iterations, 20);
        assert!(cfg.parallel_tools);
        assert!(cfg.system_prompt.is_none());
        assert!(cfg.timeout.is_none());
    }

    #[test]
    fn builder_overrides_all_fields() {
        let cfg = AgentConfig::builder()
            .model("gpt-4o")
            .system_prompt("Be concise.")
            .max_iterations(5)
            .max_tokens(512)
            .temperature(0.7)
            .sequential_tools()
            .timeout(Duration::from_secs(30))
            .build();

        assert_eq!(cfg.model, "gpt-4o");
        assert_eq!(cfg.max_iterations, 5);
        assert_eq!(cfg.max_tokens, Some(512));
        assert!((cfg.temperature.unwrap() - 0.7).abs() < f32::EPSILON);
        assert!(!cfg.parallel_tools);
        assert_eq!(cfg.timeout, Some(Duration::from_secs(30)));
    }

    #[test]
    fn system_prompt_renders_variables() {
        let prompt = SystemPrompt::new("Hello, {{name}}! You are a {{role}}.");
        let mut vars = HashMap::new();
        vars.insert("name".to_owned(), "Alice".to_owned());
        vars.insert("role".to_owned(), "coder".to_owned());
        assert_eq!(prompt.render(&vars), "Hello, Alice! You are a coder.");
    }

    #[test]
    fn system_prompt_unknown_key_is_preserved() {
        let prompt = SystemPrompt::new("Hello {{unknown}}.");
        assert_eq!(prompt.render(&HashMap::new()), "Hello {{unknown}}.");
    }

    #[test]
    fn system_prompt_from_str() {
        let p: SystemPrompt = "test".into();
        assert_eq!(p.as_str(), "test");
    }
}
