//! The `Agent` trait and `AgentFuture` type alias.

use std::{future::Future, pin::Pin};

use crate::{AgentConfig, AgentContext, AgentError};

/// The return type of [`Agent::run`]: a heap-allocated, `Send`, `'a`-lifetime future.
///
/// Using a named type alias keeps `Agent` implementations readable. The `'a`
/// lifetime is tied to `&'a self` in [`Agent::run`] so the future may borrow
/// from the agent.
pub type AgentFuture<'a, O, E> = Pin<Box<dyn Future<Output = Result<O, E>> + Send + 'a>>;

/// The core abstraction for a Kainetic agent.
///
/// An agent receives typed input, operates on a [`AgentContext`], and returns
/// typed output. In practice you rarely implement this by hand — attach
/// `#[kainetic_macros::agent]` to an `async fn` to get a generated implementation.
///
/// # Manual implementation
///
/// Manual implementations are useful when you need full control over context
/// setup, streaming output, or custom cancellation logic.
///
/// ```rust,no_run
/// use kainetic_core::{Agent, AgentConfig, AgentContext, AgentError, AgentFuture};
///
/// pub struct GreeterAgent {
///     config: AgentConfig,
/// }
///
/// impl Agent for GreeterAgent {
///     type Input = String;
///     type Output = String;
///     type Error = AgentError;
///
///     fn name(&self) -> &'static str { "greeter" }
///     fn description(&self) -> &'static str { "Greets the user." }
///     fn config(&self) -> &AgentConfig { &self.config }
///
///     fn run(
///         &self,
///         input: String,
///         _ctx: AgentContext,
///     ) -> AgentFuture<'_, String, AgentError> {
///         Box::pin(async move { Ok(format!("Hello, {input}!")) })
///     }
/// }
/// ```
pub trait Agent: Send + Sync + 'static {
    /// The input type the agent accepts.
    type Input: Send;
    /// The output type the agent produces on success.
    type Output: Send;
    /// The error type returned on failure.
    ///
    /// Must be convertible to [`AgentError`] so the runtime can map it to
    /// [`kainetic_schema::KaineticError`].
    type Error: Into<AgentError> + Send;

    /// Returns the agent's stable identifier.
    ///
    /// Used in tracing spans, event payloads, and registry lookups.
    fn name(&self) -> &'static str;

    /// Returns a human-readable description of what the agent does.
    fn description(&self) -> &'static str;

    /// Returns the agent's runtime configuration.
    fn config(&self) -> &AgentConfig;

    /// Runs the agent with the given input and execution context.
    ///
    /// Implementations should:
    /// - Check `ctx.cancellation_token.is_cancelled()` periodically.
    /// - Delegate LLM calls to `ctx.provider` via a [`crate::ReActLoop`].
    /// - Emit lifecycle events through `ctx.emit(...)`.
    fn run(
        &self,
        input: Self::Input,
        ctx: AgentContext,
    ) -> AgentFuture<'_, Self::Output, Self::Error>;
}
