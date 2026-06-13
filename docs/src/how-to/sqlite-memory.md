# Persistent Memory with SQLite

Use `SqliteBackend` to persist conversation history across process restarts.

## Setup

```toml
[dependencies]
kainetic-memory = { version = "0.1", features = ["sqlite"] }
```

```rust
use kainetic_memory::{EpisodicMemory, SqliteBackend};
use kainetic_schema::SessionId;
use std::str::FromStr;

// Open or create the database
let backend = SqliteBackend::new("agent-memory.db")?;

// Wrap in EpisodicMemory for automatic history management
let session_id = SessionId::from_str(&user_session_id)?;
let memory = EpisodicMemory::new(backend, session_id, /* max_history */ 50);

let runtime = KaineticRuntime::builder()
    .provider(AnthropicProvider::from_env()?)
    .memory(memory)
    .build();
```

## How it works

`EpisodicMemory` writes every `memory_write` call to SQLite under a key scoped to the `SessionId`. On subsequent runs with the same session ID, the stored history is automatically prepended to the conversation.

When `max_history` entries are exceeded, the oldest entries are trimmed. Future versions will summarise rather than discard.

## Multiple users

Create one `EpisodicMemory` per user session — each has its own `SessionId` and independent history:

```rust
let memory = EpisodicMemory::new(
    SqliteBackend::new("agent-memory.db")?,
    user.session_id,
    100,
);
```

The SQLite backend is safe for concurrent access from multiple threads (uses `r2d2` connection pooling).
