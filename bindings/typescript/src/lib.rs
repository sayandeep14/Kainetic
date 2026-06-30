//! Node.js bindings for Kainetic via napi-rs.
//!
//! Exposes the Kainetic runtime to Node.js / TypeScript.
//! Build with `napi build --platform`.
//!
//! # TypeScript usage
//!
//! ```typescript
//! import { KaineticRuntime, AnthropicProvider, tool, agent } from '@kainetic/runtime';
//!
//! const greet = tool(
//!   { name: 'greet', description: 'Returns a greeting.' },
//!   (jsonInput) => {
//!     const { name } = JSON.parse(jsonInput);
//!     return JSON.stringify({ greeting: `Hello, ${name}!` });
//!   }
//! );
//!
//! const hello = agent(
//!   { name: 'hello', description: 'A greeting agent.' },
//!   (input) => `The agent says: ${input}`
//! );
//!
//! const provider = AnthropicProvider.fromEnv();
//! const runtime = new KaineticRuntime({ anthropicProvider: provider, tools: [greet] });
//! const result = await runtime.run(hello, 'world');
//! console.log(result);
//! ```

// FFI crate — napi-rs exports are called from JS, not from Rust, so Rust's
// dead-code lint cannot see that #[napi] items are used.
#![allow(
    clippy::used_underscore_binding,
    clippy::needless_pass_by_value,
    dead_code
)]

mod agent;
mod provider;
mod runtime;
mod tool;

// napi-rs registers the module automatically via the #[napi] macros.
