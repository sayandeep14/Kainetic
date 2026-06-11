//! Tool descriptor type — the static metadata every tool exposes.

use schemars::schema::RootSchema;
use serde::{Deserialize, Serialize};

/// Static metadata for a registered tool.
///
/// Every value that implements the `Tool` trait (defined in `kainetic-tools`)
/// must produce a `ToolDescriptor`. The descriptor is used to:
///
/// - advertise available tools to a model provider in the correct wire format,
/// - validate tool call inputs against `input_schema` before dispatch, and
/// - generate documentation and introspection output in `kainetic-cli`.
///
/// Both schemas are [`RootSchema`] values produced by
/// [`schemars::schema_for!`] or [`schemars::gen::SchemaGenerator`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    /// Unique name the model uses when requesting this tool (`snake_case`).
    pub name: String,
    /// Human-readable description sent to the model in the tool list.
    ///
    /// Write this as if explaining the tool to the model: what it does, when
    /// to use it, and any important constraints or side effects.
    pub description: String,
    /// JSON Schema describing the expected input to this tool.
    ///
    /// Generated automatically by the `#[tool]` macro via `schemars`.
    pub input_schema: RootSchema,
    /// JSON Schema describing the output returned by this tool on success.
    ///
    /// Generated automatically by the `#[tool]` macro via `schemars`.
    pub output_schema: RootSchema,
}

impl ToolDescriptor {
    /// Constructs a [`ToolDescriptor`] from its component parts.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: RootSchema,
        output_schema: RootSchema,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
            output_schema,
        }
    }
}

#[cfg(test)]
mod tests {
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Serialize, Deserialize, JsonSchema)]
    struct SearchInput {
        /// The search query.
        query: String,
        /// Maximum number of results to return.
        max_results: Option<u8>,
    }

    #[derive(Serialize, Deserialize, JsonSchema)]
    struct SearchOutput {
        /// Matched result snippets.
        results: Vec<String>,
    }

    #[test]
    fn tool_descriptor_stores_metadata() {
        let descriptor = ToolDescriptor::new(
            "web_search",
            "Search the web for current information.",
            schemars::schema_for!(SearchInput),
            schemars::schema_for!(SearchOutput),
        );
        assert_eq!(descriptor.name, "web_search");
        assert_eq!(
            descriptor.description,
            "Search the web for current information."
        );
    }

    #[test]
    fn input_schema_is_object_type() {
        let descriptor = ToolDescriptor::new(
            "web_search",
            "Search the web.",
            schemars::schema_for!(SearchInput),
            schemars::schema_for!(SearchOutput),
        );
        let schema_value = serde_json::to_value(&descriptor.input_schema).unwrap();
        assert_eq!(schema_value["type"], "object");
        assert!(schema_value["properties"]["query"].is_object());
    }

    #[test]
    fn descriptor_serde_round_trip() {
        let d = ToolDescriptor::new(
            "noop",
            "Does nothing.",
            schemars::schema_for!(SearchInput),
            schemars::schema_for!(SearchOutput),
        );
        let json = serde_json::to_string(&d).unwrap();
        let d2: ToolDescriptor = serde_json::from_str(&json).unwrap();
        assert_eq!(d.name, d2.name);
        assert_eq!(d.description, d2.description);
    }
}
