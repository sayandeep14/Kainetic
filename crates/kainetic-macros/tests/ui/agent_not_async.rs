use kainetic_core::{AgentContext, AgentError};

#[kainetic_macros::agent(description = "Not async.")]
fn not_async_agent(input: String, _ctx: AgentContext) -> Result<String, AgentError> {
    Ok(input)
}

fn main() {}
