# Build a Custom Tool

## With `#[tool]`

```rust
use kainetic_macros::tool;
use kainetic_tools::{ToolContext, ToolError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, JsonSchema)]
pub struct TranslateInput {
    /// The text to translate.
    pub text: String,
    /// ISO 639-1 language code (e.g. "fr", "de", "ja").
    pub target_language: String,
}

#[derive(Serialize, JsonSchema)]
pub struct TranslateOutput {
    pub translated: String,
    pub detected_source_language: String,
}

#[tool(
    description = "Translates text to the specified language using an external API.",
    timeout = "10s"
)]
pub async fn translate(
    input: TranslateInput,
    ctx: ToolContext,
) -> Result<TranslateOutput, ToolError> {
    // Check for cancellation before making a network call
    if ctx.cancellation_token.is_cancelled() {
        return Err(ToolError::Cancelled);
    }

    // Call your translation API here...
    let response = call_translation_api(&input.text, &input.target_language)
        .await
        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

    Ok(TranslateOutput {
        translated: response.text,
        detected_source_language: response.source_lang,
    })
}
```

Register with:

```rust
.tool(Translate)   // generated struct is PascalCase of function name
```

## Input schema best practices

- Document every field with a rustdoc comment — the schema description is passed to the model.
- Use specific types: `u32` instead of `i64` where a negative value makes no sense.
- Use enums to constrain valid values: `pub enum Language { English, French, German }`.
- Mark optional fields with `Option<T>` and provide a serde default if appropriate.

## Error handling

| Situation | Return |
|---|---|
| Input is semantically invalid (e.g. invalid URL) | `Err(ToolError::InputValidation(msg))` |
| External call failed | `Err(ToolError::ExecutionFailed(msg))` |
| Cancellation token set | `Err(ToolError::Cancelled)` |
| Timeout (if using `timeout = "Xs"`) | Handled automatically by the macro |
