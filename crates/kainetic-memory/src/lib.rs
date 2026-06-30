//! Memory backends for Kainetic agents.
//!
//! Defines the [`MemoryBackend`] trait and concrete implementations:
//! [`InMemoryBackend`] (ephemeral), `SqliteBackend` (persistent episodic,
//! feature `sqlite`), `RedisBackend` (distributed episodic, feature
//! `redis`), and `UsearchBackend` (in-process vector search, feature
//! `usearch`). Also provides [`WorkingMemory`] and [`EpisodicMemory`]
//! wrappers with automatic context window management.
//!
//! # Choosing a backend
//!
//! | Need | Backend |
//! |------|---------|
//! | Tests / single-process | [`InMemoryBackend`] |
//! | Cross-run persistence (local) | `SqliteBackend` (feature `sqlite`) |
//! | Distributed agents | `RedisBackend` (feature `redis`) |
//! | Semantic / RAG search | `UsearchBackend` (feature `usearch`) |
#![deny(clippy::all, clippy::pedantic, missing_docs, unsafe_code)]

mod backend;
mod episodic;
mod error;
mod in_memory;
mod types;
mod working;

#[cfg(feature = "pgvector")]
mod pgvector;

#[cfg(feature = "qdrant")]
mod qdrant;

#[cfg(feature = "redis")]
mod redis_backend;

#[cfg(feature = "sqlite")]
mod sqlite;

#[cfg(feature = "usearch")]
mod usearch_backend;

pub use backend::MemoryBackend;
pub use episodic::EpisodicMemory;
pub use error::MemoryError;
pub use in_memory::InMemoryBackend;
pub use types::{MemoryEntry, MemoryEntryBuilder, MemoryKey, SemanticQuery};
pub use working::WorkingMemory;

#[cfg(feature = "pgvector")]
pub use pgvector::PgVectorBackend;

#[cfg(feature = "qdrant")]
pub use qdrant::QdrantBackend;

#[cfg(feature = "redis")]
pub use redis_backend::RedisBackend;

#[cfg(feature = "sqlite")]
pub use sqlite::SqliteBackend;

#[cfg(feature = "usearch")]
pub use usearch_backend::UsearchBackend;
