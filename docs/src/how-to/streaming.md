# Streaming Responses

Kainetic supports incremental token-by-token streaming for providers that offer SSE (Server-Sent Events) endpoints.

## Provider-level streaming

All built-in providers implement `stream()`:

```rust
use futures::StreamExt;

let mut stream = provider
    .stream(CompletionRequest::new("claude-sonnet-4-6", messages))
    .await?;

while let Some(chunk) = stream.next().await {
    match chunk? {
        CompletionChunk { delta: ChunkDelta::Text(text), .. } => {
            print!("{text}");
        }
        CompletionChunk { delta: ChunkDelta::Done { stop_reason, usage }, .. } => {
            println!("\n[done: {stop_reason:?}, tokens: {}]", usage.total_tokens);
            break;
        }
        _ => {}
    }
}
```

## Streaming in the agent loop

The `ReActLoop` currently uses `complete()` (full response) rather than `stream()`. This is intentional — the loop needs the full tool call list before it can dispatch tools.

For a streaming user experience, stream the final response after the loop completes:

```rust
// Run the ReAct loop to completion (may call tools)
let final_input = react_loop.execute(user_input, ctx.clone()).await?;

// Then stream the final synthesis to the user
let stream = provider.stream(
    CompletionRequest::new("claude-sonnet-4-6", final_messages)
).await?;
// … pipe stream to the user interface
```

A streaming-native `ReActLoop` that streams the final turn is on the roadmap.
