//! Token usage and cost tracking types.

use std::ops::{Add, AddAssign};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Token counts for a single model call.
///
/// Returned by every `ModelProvider` completion and accumulated by the
/// `CostAccumulator` in `kainetic-telemetry`.
// `_tokens` suffix on all three fields is intentional — it mirrors the names
// used by every major provider API and makes the meaning unambiguous at call sites.
#[allow(clippy::struct_field_names)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TokenUsage {
    /// Tokens consumed by the prompt (input), including system prompt and history.
    pub prompt_tokens: u32,
    /// Tokens produced by the completion (output).
    pub completion_tokens: u32,
    /// Total tokens consumed (`prompt_tokens + completion_tokens`).
    ///
    /// Providers report this directly; Kainetic does not recompute it, to
    /// preserve any provider-specific counting (e.g., cached token discounts).
    pub total_tokens: u32,
}

impl TokenUsage {
    /// Creates a [`TokenUsage`] from raw prompt and completion counts.
    ///
    /// `total_tokens` is set to `prompt_tokens + completion_tokens`.
    #[must_use]
    pub fn new(prompt_tokens: u32, completion_tokens: u32) -> Self {
        Self {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens.saturating_add(completion_tokens),
        }
    }
}

impl Add for TokenUsage {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self {
            prompt_tokens: self.prompt_tokens.saturating_add(rhs.prompt_tokens),
            completion_tokens: self.completion_tokens.saturating_add(rhs.completion_tokens),
            total_tokens: self.total_tokens.saturating_add(rhs.total_tokens),
        }
    }
}

impl AddAssign for TokenUsage {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

/// Estimated monetary cost for a model call or accumulated run.
///
/// Costs are computed by each `ModelProvider` implementation using
/// hard-coded (but updateable) per-token prices. Always treat this as
/// an estimate — provider invoices are the authoritative source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CostEstimate {
    /// Estimated cost in US dollars.
    pub usd: f64,
    /// Provider identifier (e.g. `"anthropic"`, `"openai"`).
    pub provider: String,
    /// Model identifier used to compute the cost (e.g. `"claude-sonnet-4-6"`).
    pub model: String,
}

impl CostEstimate {
    /// Constructs a new [`CostEstimate`].
    #[must_use]
    pub fn new(usd: f64, provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            usd,
            provider: provider.into(),
            model: model.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_usage_new_sets_total() {
        let u = TokenUsage::new(100, 50);
        assert_eq!(u.prompt_tokens, 100);
        assert_eq!(u.completion_tokens, 50);
        assert_eq!(u.total_tokens, 150);
    }

    #[test]
    fn token_usage_default_is_zero() {
        let u = TokenUsage::default();
        assert_eq!(u.total_tokens, 0);
    }

    #[test]
    fn token_usage_add() {
        let a = TokenUsage::new(100, 50);
        let b = TokenUsage::new(200, 75);
        let c = a + b;
        assert_eq!(c.prompt_tokens, 300);
        assert_eq!(c.completion_tokens, 125);
        assert_eq!(c.total_tokens, 425);
    }

    #[test]
    fn token_usage_add_assign() {
        let mut a = TokenUsage::new(100, 50);
        a += TokenUsage::new(10, 5);
        assert_eq!(a.total_tokens, 165);
    }

    #[test]
    fn token_usage_saturates_on_overflow() {
        let a = TokenUsage {
            prompt_tokens: u32::MAX,
            completion_tokens: 0,
            total_tokens: u32::MAX,
        };
        let b = TokenUsage::new(1, 0);
        let c = a + b;
        assert_eq!(c.prompt_tokens, u32::MAX);
    }

    #[test]
    fn token_usage_serde_round_trip() {
        let u = TokenUsage::new(512, 128);
        let json = serde_json::to_string(&u).unwrap();
        let u2: TokenUsage = serde_json::from_str(&json).unwrap();
        assert_eq!(u, u2);
    }

    #[test]
    fn cost_estimate_serde_round_trip() {
        let c = CostEstimate::new(0.003, "anthropic", "claude-sonnet-4-6");
        let json = serde_json::to_string(&c).unwrap();
        let c2: CostEstimate = serde_json::from_str(&json).unwrap();
        assert_eq!(c, c2);
    }
}
