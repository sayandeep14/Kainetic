# Migration from LangChain / LangGraph

This guide maps LangChain and LangGraph concepts to their Kainetic equivalents and shows side-by-side code for common patterns.

## Concept mapping

| LangChain / LangGraph | Kainetic |
|---|---|
| `BaseTool` / `@tool` | `Tool` trait / `#[tool]` |
| `AgentExecutor` | `KaineticRuntime` + `ReActLoop` |
| `BaseAgent` | `Agent` trait / `#[agent]` |
| `LLMChain` | `ReActLoop` without tools |
| `ConversationChain` | `EpisodicMemory` + `ReActLoop` |
| `VectorStoreRetriever` | `UsearchBackend` / `QdrantBackend` |
| `StateGraph` | `Pipeline` / `StateMachineAgent` |
| `CompiledGraph` | `Pipeline::build()` |
| `MessageHistory` | `EpisodicMemory` |
| `Callbacks` | `AgentEvent` broadcast channel |
| `LangSmith` | Kainetic Cloud + OTLP traces |

## Tool definition

**LangChain (Python)**
```python
from langchain.tools import tool

@tool
def add(a: float, b: float) -> float:
    """Adds two numbers."""
    return a + b
```

**Kainetic (Rust)**
```rust
#[tool(description = "Adds two numbers.")]
async fn add(input: AddInput, _ctx: ToolContext) -> Result<AddOutput, ToolError> {
    Ok(AddOutput { sum: input.a + input.b })
}
```

Key differences:
- Input/output are typed structs, not bare scalars.
- JSON Schema validation runs **before** your code executes.
- The function is `async` — I/O is natural, no `asyncio.run` bridge needed.

## Agent definition

**LangChain (Python)**
```python
from langchain.agents import AgentExecutor, create_react_agent

agent = create_react_agent(llm, tools, prompt)
executor = AgentExecutor(agent=agent, tools=tools)
result = executor.invoke({"input": "What day is it?"})
```

**Kainetic (Rust)**
```rust
#[agent(description = "Answers questions using tools.")]
async fn assistant(input: String, ctx: AgentContext) -> Result<String, AgentError> {
    ReActLoop::new(ctx.config().clone()).execute(input, ctx).await
}

let result = runtime.run(&Assistant::new(), "What day is it?".into()).await?;
```

Key differences:
- The agent body is just an `async fn` — no class hierarchy.
- No separate `AgentExecutor` — the runtime and loop are one concept.
- Cancellation is structural, not `asyncio.CancelledError`.

## Memory

**LangChain (Python)**
```python
from langchain.memory import ConversationBufferMemory
memory = ConversationBufferMemory()
chain = LLMChain(llm=llm, memory=memory)
```

**Kainetic (Rust)**
```rust
use kainetic_memory::{EpisodicMemory, SqliteBackend};

let memory = SqliteBackend::new("agent.db")?;
let runtime = KaineticRuntime::builder()
    .provider(provider)
    .memory(EpisodicMemory::new(memory, session_id, 50))
    .build();
```

Kainetic memory is per-run by default and persisted with a SQLite or Redis backend. The `EpisodicMemory` wrapper handles history trimming and context window management automatically.

## Multi-agent (LangGraph → Pipeline)

**LangGraph (Python)**
```python
from langgraph.graph import StateGraph

workflow = StateGraph(MyState)
workflow.add_node("researcher", researcher_agent)
workflow.add_node("writer", writer_agent)
workflow.add_edge("researcher", "writer")
app = workflow.compile()
```

**Kainetic (Rust)**
```rust
let pipeline = Pipeline::builder()
    .agent("researcher", ResearcherAgent::new())
    .agent("writer", WriterAgent::new())
    .edge("researcher", "writer", |output| WriterInput { draft: output.findings })
    .build()?;

let result = pipeline.run(PipelineInput { query: "..." }, ctx).await?;
```

Key differences:
- Edges carry typed transforms — no stringly-typed `State` dict.
- Pipeline validation (reachability, type compatibility) happens at `build()` time, not at first call.

## Performance comparison

| Scenario | LangChain (Python) | Kainetic (Rust) |
|---|---|---|
| Cold start | ~120 ms | < 5 ms |
| Agent at rest | ~200 MB RSS | < 15 MB RSS |
| 100 concurrent runs | ~8 GB | < 300 MB |
| P99 LLM latency overhead | +800 ms | < 50 ms |

*Benchmarks run on Apple M3 Max, Claude Sonnet 4.6, 1 tool call per run.*
