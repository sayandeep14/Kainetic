//! Typed identity newtypes for Kainetic runtime entities.
//!
//! Each ID is a `#[repr(transparent)]` newtype over [`uuid::Uuid`], giving
//! compile-time separation between run IDs, session IDs, agent IDs, and tool
//! IDs while serialising identically to a plain UUID string.

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! define_id {
    (
        $(#[$attr:meta])*
        $name:ident
    ) => {
        $(#[$attr])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Generates a new, randomly-generated ID using UUID v4.
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// Returns the underlying [`Uuid`].
            #[must_use]
            pub fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<Uuid> for $name {
            fn from(uuid: Uuid) -> Self {
                Self(uuid)
            }
        }

        impl From<$name> for Uuid {
            fn from(id: $name) -> Uuid {
                id.0
            }
        }
    };
}

define_id! {
    /// Unique identifier for a single agent run (one call to `KaineticRuntime::run`).
    RunId
}

define_id! {
    /// Unique identifier for a conversation session, spanning multiple runs.
    SessionId
}

define_id! {
    /// Unique identifier for a registered agent definition.
    AgentId
}

define_id! {
    /// Unique identifier for a registered tool definition.
    ToolId
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_id_is_unique() {
        assert_ne!(RunId::new(), RunId::new());
    }

    #[test]
    fn session_id_display_is_uuid_format() {
        let id = SessionId::new();
        // UUID v4 string has 36 chars: 8-4-4-4-12 with hyphens
        assert_eq!(id.to_string().len(), 36);
    }

    #[test]
    fn agent_id_round_trips_through_uuid() {
        let id = AgentId::new();
        let uuid: Uuid = id.into();
        let id2: AgentId = uuid.into();
        assert_eq!(id, id2);
    }

    #[test]
    fn tool_id_serde_round_trip() {
        let id = ToolId::new();
        let json = serde_json::to_string(&id).unwrap();
        let id2: ToolId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, id2);
    }

    #[test]
    fn run_id_serialises_as_plain_string() {
        let id = RunId::new();
        let json = serde_json::to_string(&id).unwrap();
        // Transparent serde — serialises as a quoted UUID string, not `{"0":"..."}`.
        assert!(json.starts_with('"'));
        assert!(json.ends_with('"'));
    }

    #[test]
    fn default_produces_valid_id() {
        let id = RunId::default();
        // Should not panic and should produce a non-nil UUID.
        assert_ne!(id.as_uuid(), Uuid::nil());
    }
}
