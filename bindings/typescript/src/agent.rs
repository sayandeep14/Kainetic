//! TypeScript-facing `agent()` factory.

use std::sync::{Arc, Mutex};

use kainetic_core::{Agent, AgentConfig, AgentContext, AgentError, AgentFuture};
use napi::bindgen_prelude::*;
use napi::threadsafe_function::{
    ErrorStrategy, ThreadSafeCallContext, ThreadsafeFunction, ThreadsafeFunctionCallMode,
};
use napi_derive::napi;

type AgentTsfn = ThreadsafeFunction<String, ErrorStrategy::Fatal>;

/// A Kainetic agent backed by a synchronous JavaScript function.
///
/// The JS function receives a string input and must return a string output
/// synchronously.
pub struct JsAgent {
    agent_name: String,
    agent_description: String,
    config: AgentConfig,
    tsfn: Arc<AgentTsfn>,
}

impl Agent for JsAgent {
    type Input = String;
    type Output = String;
    type Error = AgentError;

    fn name(&self) -> &'static str {
        Box::leak(self.agent_name.clone().into_boxed_str())
    }

    fn description(&self) -> &'static str {
        Box::leak(self.agent_description.clone().into_boxed_str())
    }

    fn config(&self) -> &AgentConfig {
        &self.config
    }

    fn run(&self, input: String, _ctx: AgentContext) -> AgentFuture<'_, String, AgentError> {
        let tsfn = Arc::clone(&self.tsfn);
        Box::pin(async move {
            let (tx, rx) = tokio::sync::oneshot::channel::<String>();
            let shared_tx = Arc::new(Mutex::new(Some(tx)));
            let cb_tx = Arc::clone(&shared_tx);

            tsfn.call_with_return_value::<napi::JsString, _>(
                input,
                ThreadsafeFunctionCallMode::NonBlocking,
                move |js_str: napi::JsString| {
                    let s = js_str.into_utf8()?.as_str()?.to_owned();
                    if let Some(sender) = cb_tx.lock().unwrap().take() {
                        let _ = sender.send(s);
                    }
                    Ok(())
                },
            );

            rx.await
                .map_err(|_| AgentError::User("JS agent channel dropped".into()))
        })
    }
}

/// Options for the `agent()` factory function.
#[napi(object)]
pub struct AgentOptions {
    /// Agent name.
    pub name: String,
    /// One-sentence description.
    pub description: String,
}

/// A handle to a JS-backed Kainetic agent.
#[napi]
pub struct AgentHandle {
    pub(crate) inner: Arc<JsAgent>,
}

/// Create a Kainetic agent from a JavaScript function.
///
/// The function receives a string input and must return a string output
/// synchronously.
///
/// @param options - Agent metadata (name and description).
/// @param fn_     - Function `(input: string) => string`.
/// @returns An `AgentHandle` to pass to `KaineticRuntime.run()`.
///
/// @example
/// ```typescript
/// const echo = agent(
///   { name: 'echo', description: 'Echoes input.' },
///   (input) => input
/// );
/// ```
#[napi]
pub fn agent(_env: Env, options: AgentOptions, fn_: JsFunction) -> napi::Result<AgentHandle> {
    let tsfn: AgentTsfn =
        fn_.create_threadsafe_function(0, |ctx: ThreadSafeCallContext<String>| {
            let js_str = ctx.env.create_string(&ctx.value)?;
            Ok(vec![js_str.into_unknown()])
        })?;

    Ok(AgentHandle {
        inner: Arc::new(JsAgent {
            agent_name: options.name,
            agent_description: options.description,
            config: AgentConfig::builder().build(),
            tsfn: Arc::new(tsfn),
        }),
    })
}
