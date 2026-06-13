# Memory Backends

Kainetic's memory system is pluggable. Any type implementing `MemoryBackend` can back an agent's memory. The default is `InMemoryBackend` — volatile, zero-configuration, no persistence.

## The `MemoryBackend` trait

```rust
#[async_trait]
pub trait MemoryBackend: Send + Sync + 'static {
    async fn read(&self, key: &MemoryKey) -> Result<Option<MemoryEntry>, MemoryError>;
    async fn write(&self, key: MemoryKey, entry: MemoryEntry) -> Result<(), MemoryError>;
    async fn delete(&self, key: &MemoryKey) -> Result<(), MemoryError>;
    async fn search(&self, query: &SemanticQuery) -> Result<Vec<MemoryEntry>, MemoryError>;
    async fn flush(&self) -> Result<(), MemoryError>;
}
```

## Available backends

| Backend | Crate feature | Persistence | Semantic search |
|---|---|---|---|
| `InMemoryBackend` | (default) | No | No |
| `SqliteBackend` | `sqlite` | Yes | No |
| `RedisBackend` | `redis` | Yes | No |
| `UsearchBackend` | `usearch` | No | Yes (HNSW) |
| `PgVectorBackend` | `pgvector` | Yes | Yes |
| `QdrantBackend` | `qdrant` | Yes | Yes |

## Working memory vs episodic memory

**`WorkingMemory`** — scoped to a single `RunId`. Values are automatically cleared when the run ends. Use for temporary reasoning state within a run.

**`EpisodicMemory`** — scoped to a `SessionId`. Persists conversation history across runs. Automatically trims the oldest turns when the history exceeds `max_history` entries.

```rust
use kainetic_memory::{EpisodicMemory, SqliteBackend};

let backend = SqliteBackend::new("agent-memory.db")?;
let memory = EpisodicMemory::new(backend, session_id, /* max_history */ 50);

let runtime = KaineticRuntime::builder()
    .memory(memory)
    .build();
```

## Semantic search

Use `UsearchBackend` (in-process HNSW) or `QdrantBackend` (hosted vector store) for semantic retrieval. Store documents with pre-computed embeddings:

```rust
use kainetic_memory::{MemoryEntry, MemoryKey, SemanticQuery};

ctx.memory_write(
    MemoryKey::new("docs", "readme"),
    MemoryEntry::builder("Kainetic is a Rust runtime for AI agents.")
        .embedding(embed("Kainetic is a Rust runtime for AI agents.").await?)
        .build(),
).await?;

let results = ctx.memory.search(&SemanticQuery::new("what is kainetic")
    .embedding(embed("what is kainetic").await?)
    .top_k(5)
    .threshold(0.7)
).await?;
```

Kainetic does not include an embedding model — bring your own via your provider's embeddings API or a local model.
