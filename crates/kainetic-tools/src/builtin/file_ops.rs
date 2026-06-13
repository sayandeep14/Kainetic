//! [`FileReadTool`] and [`FileWriteTool`] — safe local file I/O.

use std::path::{Path, PathBuf};

use kainetic_schema::RootSchema;
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::{Tool, ToolContext, ToolError, ToolFuture};

// ─── FileReadTool ──────────────────────────────────────────────────────────────

/// Reads a local file and returns its contents as a UTF-8 string.
///
/// Optionally limits the output to a line range.  The tool rejects paths
/// containing `..` (path traversal) and symlinks that escape the allowed root.
pub struct FileReadTool {
    /// Allowed root directory; `None` means current working directory only.
    allowed_root: Option<PathBuf>,
}

impl FileReadTool {
    /// Creates a new `FileReadTool` with no root restriction.
    #[must_use]
    pub fn new() -> Self {
        Self { allowed_root: None }
    }

    /// Creates a `FileReadTool` restricted to `root` and its subdirectories.
    #[must_use]
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self {
            allowed_root: Some(root.into()),
        }
    }

    fn check_path(&self, path: &Path) -> Result<PathBuf, ToolError> {
        // Reject literal `..` components before canonicalisation.
        if path.components().any(|c| c.as_os_str() == "..") {
            return Err(ToolError::InputValidation(
                "path traversal with '..' is not permitted".into(),
            ));
        }
        let canonical = std::fs::canonicalize(path)
            .map_err(|e| ToolError::ExecutionFailed(format!("cannot resolve path: {e}")))?;
        if let Some(root) = &self.allowed_root {
            let canonical_root = std::fs::canonicalize(root)
                .map_err(|e| ToolError::ExecutionFailed(format!("cannot resolve root: {e}")))?;
            if !canonical.starts_with(&canonical_root) {
                return Err(ToolError::InputValidation(format!(
                    "path '{}' is outside the allowed root '{}'",
                    canonical.display(),
                    canonical_root.display()
                )));
            }
        }
        Ok(canonical)
    }
}

impl Default for FileReadTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Deserialize, JsonSchema)]
struct ReadInput {
    path: String,
    start_line: Option<usize>,
    end_line: Option<usize>,
}

#[derive(Serialize, JsonSchema)]
struct ReadOutput {
    path: String,
    content: String,
    line_count: usize,
}

impl Tool for FileReadTool {
    fn name(&self) -> &'static str {
        "file_read"
    }

    fn description(&self) -> &'static str {
        "Read the contents of a local file. Supports optional line range selection."
    }

    fn input_schema(&self) -> RootSchema {
        schema_for!(ReadInput)
    }

    fn output_schema(&self) -> RootSchema {
        schema_for!(ReadOutput)
    }

    fn call(&self, input: serde_json::Value, _ctx: ToolContext) -> ToolFuture<'_> {
        Box::pin(async move {
            let params: ReadInput = serde_json::from_value(input)
                .map_err(|e| ToolError::InputValidation(e.to_string()))?;

            let path = Path::new(&params.path);
            let canonical = self.check_path(path)?;
            debug!(path = %canonical.display(), "file_read");

            let raw = tokio::fs::read_to_string(&canonical)
                .await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

            let lines: Vec<&str> = raw.lines().collect();
            let total = lines.len();

            let start = params.start_line.unwrap_or(1).saturating_sub(1);
            let end = params.end_line.map_or(total, |e| e.min(total));
            let content = lines[start.min(total)..end].join("\n");

            serde_json::to_value(ReadOutput {
                path: canonical.display().to_string(),
                content,
                line_count: total,
            })
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
        })
    }
}

// ─── FileWriteTool ─────────────────────────────────────────────────────────────

/// Writes content to a local file.  Supports both overwrite and append modes.
pub struct FileWriteTool {
    allowed_root: Option<PathBuf>,
}

impl FileWriteTool {
    /// Creates a new `FileWriteTool` with no root restriction.
    #[must_use]
    pub fn new() -> Self {
        Self { allowed_root: None }
    }

    /// Creates a `FileWriteTool` restricted to `root` and its subdirectories.
    #[must_use]
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self {
            allowed_root: Some(root.into()),
        }
    }

    fn check_path_write(&self, path: &Path) -> Result<PathBuf, ToolError> {
        if path.components().any(|c| c.as_os_str() == "..") {
            return Err(ToolError::InputValidation(
                "path traversal with '..' is not permitted".into(),
            ));
        }
        let abs = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?
                .join(path)
        };
        if let Some(root) = &self.allowed_root {
            let canonical_root = std::fs::canonicalize(root)
                .map_err(|e| ToolError::ExecutionFailed(format!("cannot resolve root: {e}")))?;
            if !abs.starts_with(&canonical_root) {
                return Err(ToolError::InputValidation(format!(
                    "path '{}' is outside the allowed root '{}'",
                    abs.display(),
                    canonical_root.display()
                )));
            }
        }
        Ok(abs)
    }
}

impl Default for FileWriteTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Deserialize, JsonSchema)]
struct WriteInput {
    path: String,
    content: String,
    #[serde(default)]
    append: bool,
    #[serde(default)]
    create_dirs: bool,
}

#[derive(Serialize, JsonSchema)]
struct WriteOutput {
    path: String,
    bytes_written: usize,
}

impl Tool for FileWriteTool {
    fn name(&self) -> &'static str {
        "file_write"
    }

    fn description(&self) -> &'static str {
        "Write content to a local file.  Set 'append' to true to append instead of overwrite."
    }

    fn input_schema(&self) -> RootSchema {
        schema_for!(WriteInput)
    }

    fn output_schema(&self) -> RootSchema {
        schema_for!(WriteOutput)
    }

    fn call(&self, input: serde_json::Value, _ctx: ToolContext) -> ToolFuture<'_> {
        Box::pin(async move {
            let params: WriteInput = serde_json::from_value(input)
                .map_err(|e| ToolError::InputValidation(e.to_string()))?;

            let path = self.check_path_write(Path::new(&params.path))?;
            debug!(path = %path.display(), append = params.append, "file_write");

            if params.create_dirs {
                if let Some(parent) = path.parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                }
            }

            let bytes = params.content.as_bytes();
            if params.append {
                use tokio::io::AsyncWriteExt;
                let mut file = tokio::fs::OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(&path)
                    .await
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                file.write_all(bytes)
                    .await
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
            } else {
                tokio::fs::write(&path, bytes)
                    .await
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
            }

            serde_json::to_value(WriteOutput {
                path: path.display().to_string(),
                bytes_written: bytes.len(),
            })
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
        })
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use kainetic_schema::RunId;
    use tokio_util::sync::CancellationToken;

    fn ctx() -> ToolContext {
        ToolContext::new(RunId::new(), CancellationToken::new())
    }

    #[tokio::test]
    async fn read_write_roundtrip() {
        let dir = tempdir();
        let path = dir.join("test.txt");

        let writer = FileWriteTool::new();
        writer
            .call(
                serde_json::json!({
                    "path": path.display().to_string(),
                    "content": "hello\nworld\n"
                }),
                ctx(),
            )
            .await
            .unwrap();

        let reader = FileReadTool::new();
        let result = reader
            .call(
                serde_json::json!({ "path": path.display().to_string() }),
                ctx(),
            )
            .await
            .unwrap();

        assert_eq!(result["content"], "hello\nworld");
        assert_eq!(result["line_count"], 2);
    }

    #[tokio::test]
    async fn read_line_range() {
        let dir = tempdir();
        let path = dir.join("lines.txt");
        tokio::fs::write(&path, "a\nb\nc\nd\n").await.unwrap();

        let reader = FileReadTool::new();
        let result = reader
            .call(
                serde_json::json!({
                    "path": path.display().to_string(),
                    "start_line": 2,
                    "end_line": 3
                }),
                ctx(),
            )
            .await
            .unwrap();

        assert_eq!(result["content"], "b\nc");
    }

    #[tokio::test]
    async fn write_append() {
        let dir = tempdir();
        let path = dir.join("append.txt");
        let writer = FileWriteTool::new();

        writer
            .call(
                serde_json::json!({ "path": path.display().to_string(), "content": "line1\n" }),
                ctx(),
            )
            .await
            .unwrap();
        writer
            .call(
                serde_json::json!({ "path": path.display().to_string(), "content": "line2\n", "append": true }),
                ctx(),
            )
            .await
            .unwrap();

        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, "line1\nline2\n");
    }

    #[test]
    fn rejects_path_traversal() {
        let tool = FileReadTool::new();
        let result = tool.check_path(Path::new("../etc/passwd"));
        assert!(result.is_err());
    }

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kainetic_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
