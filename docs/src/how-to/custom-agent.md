# Build a Custom Agent

## With `#[agent]`

The `#[agent]` macro is the fastest way to define an agent from an async function:

```rust
use kainetic_core::{AgentContext, AgentError, ReActLoop};
use kainetic_macros::agent;

#[agent(description = "A research agent that searches the web and summarises results.")]
pub async fn researcher(query: String, ctx: AgentContext) -> Result<String, AgentError> {
    let config = ctx.config().clone();
    ReActLoop::new(config).execute(query, ctx).await
}
```

## Implementing `Agent` manually

For custom control over the execution model:

```rust
use kainetic_core::{Agent, AgentConfig, AgentContext, AgentError, AgentFuture, ReActLoop};

pub struct MyAgent {
    pub config: AgentConfig,
}

impl Agent for MyAgent {
    type Input  = String;
    type Output = String;
    type Error  = AgentError;

    fn name(&self) -> &'static str { "my_agent" }
    fn description(&self) -> &'static str { "Does something useful." }
    fn config(&self) -> &AgentConfig { &self.config }

    fn run<'a>(&'a self, input: String, ctx: AgentContext) -> AgentFuture<'a, String, AgentError> {
        Box::pin(async move {
            // Pre-processing: store input in memory
            ctx.memory_write(
                kainetic_memory::MemoryKey::new("run", "last_input"),
                kainetic_memory::MemoryEntry::builder(&input).build(),
            ).await?;

            // Core execution
            let result = ReActLoop::new(self.config.clone())
                .execute(input, ctx.clone())
                .await?;

            // Post-processing
            ctx.memory_write(
                kainetic_memory::MemoryKey::new("run", "last_output"),
                kainetic_memory::MemoryEntry::builder(&result).build(),
            ).await?;

            Ok(result)
        })
    }
}
```

## Structured input/output

Agents can use any type that is `Send + 'static`:

```rust
#[derive(serde::Deserialize)]
pub struct ResearchQuery {
    pub topic: String,
    pub max_sources: u32,
}

#[derive(serde::Serialize)]
pub struct ResearchReport {
    pub summary: String,
    pub sources: Vec<String>,
}

#[agent(description = "Produces a structured research report.")]
pub async fn researcher(
    input: ResearchQuery,
    ctx: AgentContext,
) -> Result<ResearchReport, AgentError> {
    // …
}
```
