# Multi-Agent Pipelines

`Pipeline` represents a directed acyclic graph of agents. Nodes are agents; edges carry typed transforms from one agent's output to the next agent's input.

## Building a pipeline

```rust
use kainetic_orchestra::Pipeline;

let pipeline = Pipeline::builder()
    .agent("researcher", ResearcherAgent::new())
    .agent("writer",     WriterAgent::new())
    .agent("reviewer",   ReviewerAgent::new())
    .edge("researcher", "writer", |research: ResearchOutput| {
        WriterInput { topic: research.query, facts: research.findings }
    })
    .edge("writer", "reviewer", |draft: WriterOutput| {
        ReviewerInput { draft: draft.text }
    })
    .build()?;  // validates DAG: no cycles, no unreachable nodes
```

## Running a pipeline

```rust
let result = pipeline
    .run(PipelineInput { query: "Explain Kainetic".into() }, ctx)
    .await?;
```

## Conditional routing

```rust
.conditional_edge("reviewer", |output: ReviewerOutput| {
    if output.approved {
        "publish"
    } else {
        "writer"  // feedback loop back to writer
    }
})
```

## Parallel agents

Use the `parallel!` macro for concurrent execution when agents are independent:

```rust
use kainetic_orchestra::parallel;

let (research, images) = parallel!(
    researcher.run(query.clone(), ctx.clone()),
    image_searcher.run(query.clone(), ctx.clone()),
).await;
```

## Supervisor pattern

Route work across a pool of identical workers:

```rust
use kainetic_orchestra::{RoutingStrategy, Supervisor};

let supervisor = Supervisor::builder()
    .workers(vec![
        SummaryAgent::new(),
        SummaryAgent::new(),
        SummaryAgent::new(),
    ])
    .routing(RoutingStrategy::LeastLoaded)
    .max_retries(2)
    .build();

let result = supervisor.dispatch(input, ctx).await?;
```

## StateMachineAgent

For long-running workflows that must survive process restarts:

```rust
use kainetic_orchestra::StateMachineAgent;

let agent = StateMachineAgent::builder()
    .memory(SqliteBackend::new("workflow.db")?)
    .build(OrderProcessingAgent::new());

// Resumes from last checkpoint if the process was interrupted
let result = agent.run(order_id, ctx).await?;
```
