//! Provider error type and its conversion to [`KaineticError`].

use std::time::Duration;

use kainetic_schema::KaineticError;
use thiserror::Error;

/// An error returned by a [`ModelProvider`] implementation.
///
/// [`ModelProvider`]: crate::ModelProvider
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ProviderError {
    /// The API rate-limited the request.
    ///
    /// `retry_after` is the server-suggested wait duration parsed from the
    /// `Retry-After` response header, if present.
    #[error("rate limited; retry after {retry_after:?}")]
    RateLimited {
        /// Suggested wait duration before the next attempt.
        retry_after: Option<Duration>,
    },

    /// The API key was missing or rejected.
    #[error("authentication failed: check your API key")]
    AuthFailed,

    /// The requested model does not exist or is not accessible.
    #[error("model not found: {0}")]
    ModelNotFound(String),

    /// The conversation exceeds the model's context window.
    #[error("context length exceeded: model limit {limit} tokens, request was {actual} tokens")]
    ContextLengthExceeded {
        /// The model's maximum context length in tokens.
        limit: u32,
        /// The actual token count of the request.
        actual: u32,
    },

    /// The provider returned a non-retryable HTTP error.
    #[error("API error {status}: {message}")]
    ApiError {
        /// HTTP status code.
        status: u16,
        /// Provider error message from the response body.
        message: String,
    },

    /// A network-level error occurred before a response was received.
    #[error("network error: {0}")]
    NetworkError(String),

    /// The provider returned a response that could not be parsed.
    #[error("deserialization error: {0}")]
    DeserializationError(String),
}

impl From<ProviderError> for KaineticError {
    fn from(e: ProviderError) -> Self {
        KaineticError::provider(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_failed_display() {
        let e = ProviderError::AuthFailed;
        assert!(e.to_string().contains("authentication failed"));
    }

    #[test]
    fn rate_limited_display_with_retry_after() {
        let e = ProviderError::RateLimited {
            retry_after: Some(Duration::from_secs(30)),
        };
        assert!(e.to_string().contains("30s"));
    }

    #[test]
    fn context_length_exceeded_display() {
        let e = ProviderError::ContextLengthExceeded {
            limit: 200_000,
            actual: 210_000,
        };
        let msg = e.to_string();
        assert!(msg.contains("200000"));
        assert!(msg.contains("210000"));
    }

    #[test]
    fn converts_to_kainetic_error() {
        let provider_err = ProviderError::AuthFailed;
        let kainetic_err: KaineticError = provider_err.into();
        assert!(kainetic_err.to_string().contains("provider error"));
    }
}
