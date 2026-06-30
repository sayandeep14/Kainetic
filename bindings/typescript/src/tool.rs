//! TypeScript-facing `tool()` factory.

use std::sync::{Arc, Mutex};

use kainetic_schema::RootSchema;
use kainetic_tools::{Tool, ToolContext, ToolError, ToolFuture};
use napi::bindgen_prelude::*;
use napi::threadsafe_function::{
    ErrorStrategy, ThreadSafeCallContext, ThreadsafeFunction, ThreadsafeFunctionCallMode,
};
use napi_derive::napi;
use schemars::schema_for;
use serde_json::Value;

type ToolTsfn = ThreadsafeFunction<String, ErrorStrategy::Fatal>;

/// A Kainetic tool backed by a synchronous JavaScript function.
///
/// The JS function receives a JSON string (stringified input) and must return a
/// JSON string synchronously.
#[derive(Clone)]
pub struct JsTool {
    tool_name: String,
    tool_description: String,
    tsfn: Arc<ToolTsfn>,
}

impl Tool for JsTool {
    fn name(&self) -> &'static str {
        Box::leak(self.tool_name.clone().into_boxed_str())
    }

    fn description(&self) -> &'static str {
        Box::leak(self.tool_description.clone().into_boxed_str())
    }

    fn input_schema(&self) -> RootSchema {
        schema_for!(Value)
    }

    fn output_schema(&self) -> RootSchema {
        schema_for!(Value)
    }

    fn call(&self, input: Value, _ctx: ToolContext) -> ToolFuture<'_> {
        let tsfn = Arc::clone(&self.tsfn);
        Box::pin(async move {
            let (tx, rx) = tokio::sync::oneshot::channel::<String>();
            let shared_tx = Arc::new(Mutex::new(Some(tx)));
            let cb_tx = Arc::clone(&shared_tx);

            let input_str = serde_json::to_string(&input)
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

            tsfn.call_with_return_value::<napi::JsString, _>(
                input_str,
                ThreadsafeFunctionCallMode::NonBlocking,
                move |js_str: napi::JsString| {
                    let s = js_str.into_utf8()?.as_str()?.to_owned();
                    if let Some(sender) = cb_tx.lock().unwrap().take() {
                        let _ = sender.send(s);
                    }
                    Ok(())
                },
            );

            let json_str = rx
                .await
                .map_err(|_| ToolError::ExecutionFailed("JS tool channel dropped".into()))?;

            serde_json::from_str(&json_str).map_err(|e| ToolError::ExecutionFailed(e.to_string()))
        })
    }
}

/// Options for the `tool()` factory function.
#[napi(object)]
pub struct ToolOptions {
    /// Tool name shown to the model.
    pub name: String,
    /// One-sentence description shown to the model.
    pub description: String,
}

/// A handle to a JS-backed Kainetic tool.
#[napi]
pub struct ToolHandle {
    pub(crate) inner: Arc<JsTool>,
}

/// Create a Kainetic tool from a JavaScript function.
///
/// The function receives a JSON string as input and must return a JSON string.
///
/// @param options - Tool metadata (name and description).
/// @param fn_     - Function `(jsonInput: string) => string`.
/// @returns A `ToolHandle` to pass to `KaineticRuntime`.
///
/// @example
/// ```typescript
/// const add = tool(
///   { name: 'add', description: 'Adds a and b.' },
///   (jsonInput) => {
///     const { a, b } = JSON.parse(jsonInput);
///     return JSON.stringify({ sum: a + b });
///   }
/// );
/// ```
#[napi]
pub fn tool(_env: Env, options: ToolOptions, fn_: JsFunction) -> napi::Result<ToolHandle> {
    let tsfn: ToolTsfn =
        fn_.create_threadsafe_function(0, |ctx: ThreadSafeCallContext<String>| {
            let js_str = ctx.env.create_string(&ctx.value)?;
            Ok(vec![js_str.into_unknown()])
        })?;

    Ok(ToolHandle {
        inner: Arc::new(JsTool {
            tool_name: options.name,
            tool_description: options.description,
            tsfn: Arc::new(tsfn),
        }),
    })
}
