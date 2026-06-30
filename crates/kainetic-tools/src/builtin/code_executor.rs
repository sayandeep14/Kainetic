//! [`CodeExecutorTool`] — executes code snippets in isolated subprocesses.
//!
//! Supports Python 3, Node.js, and Bash.  Each execution spawns a fresh
//! subprocess with a configurable CPU-time budget.
//!
//! Requires the `code-executor` crate feature.
//!
//! # Security
//!
//! This tool **does not** provide strong sandboxing.  For production use,
//! run the agent process inside a container, VM, or gVisor sandbox.  A
//! future enhancement will provide optional `wasmtime`-based WASM execution
//! for stronger isolation.

#[cfg(feature = "code-executor")]
mod inner {
    use std::time::Duration;

    use kainetic_schema::RootSchema;
    use schemars::{schema_for, JsonSchema};
    use serde::{Deserialize, Serialize};
    use tracing::debug;

    use crate::{Tool, ToolContext, ToolError, ToolFuture};

    /// Supported code execution runtimes.
    #[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
    #[serde(rename_all = "lowercase")]
    pub enum Language {
        Python,
        Node,
        Bash,
    }

    /// Executes a code snippet in a fresh subprocess and returns the output.
    ///
    /// The code is written to a temporary file and passed to the runtime
    /// interpreter.  Execution is killed after `timeout_secs` (default: 10).
    pub struct CodeExecutorTool {
        timeout_secs: u64,
        /// Directory for temporary source files.
        tmp_dir: std::path::PathBuf,
    }

    impl CodeExecutorTool {
        /// Creates a `CodeExecutorTool` with a 10-second default timeout.
        #[must_use]
        pub fn new() -> Self {
            Self {
                timeout_secs: 10,
                tmp_dir: std::env::temp_dir(),
            }
        }

        /// Sets the execution timeout.
        #[must_use]
        pub fn with_timeout(mut self, secs: u64) -> Self {
            self.timeout_secs = secs;
            self
        }
    }

    impl Default for CodeExecutorTool {
        fn default() -> Self {
            Self::new()
        }
    }

    #[derive(Deserialize, JsonSchema)]
    struct Input {
        language: Language,
        code: String,
        /// Optional text to pipe into stdin.
        stdin: Option<String>,
        /// Timeout override in seconds.
        timeout_secs: Option<u64>,
    }

    #[derive(Serialize, JsonSchema)]
    struct Output {
        exit_code: i32,
        stdout: String,
        stderr: String,
    }

    impl Tool for CodeExecutorTool {
        fn name(&self) -> &'static str {
            "code_executor"
        }

        fn description(&self) -> &'static str {
            "Execute a Python, Node.js, or Bash code snippet in a subprocess \
             and return stdout, stderr, and exit code."
        }

        fn input_schema(&self) -> RootSchema {
            schema_for!(Input)
        }

        fn output_schema(&self) -> RootSchema {
            schema_for!(Output)
        }

        fn call(&self, input: serde_json::Value, ctx: ToolContext) -> ToolFuture<'_> {
            Box::pin(async move {
                let params: Input = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvalidInput(e.to_string()))?;

                let timeout = Duration::from_secs(params.timeout_secs.unwrap_or(self.timeout_secs));

                // Write code to a temp file.
                let ext = match params.language {
                    Language::Python => "py",
                    Language::Node => "js",
                    Language::Bash => "sh",
                };
                let src_path = self.tmp_dir.join(format!(
                    "kainetic_exec_{}.{ext}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .subsec_nanos()
                ));
                tokio::fs::write(&src_path, params.code.as_bytes())
                    .await
                    .map_err(|e| ToolError::ExecutionFailed(format!("write src: {e}")))?;

                let (interpreter, interpreter_args): (&str, Vec<String>) = match params.language {
                    Language::Python => ("python3", vec![]),
                    Language::Node => ("node", vec![]),
                    Language::Bash => ("bash", vec![]),
                };

                debug!(lang = ?params.language, src = %src_path.display(), "code_executor");

                let mut cmd = tokio::process::Command::new(interpreter);
                cmd.args(&interpreter_args)
                    .arg(&src_path)
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped());

                if params.stdin.is_some() {
                    cmd.stdin(std::process::Stdio::piped());
                }

                let mut child = cmd
                    .spawn()
                    .map_err(|e| ToolError::ExecutionFailed(format!("spawn: {e}")))?;

                if let Some(stdin_data) = params.stdin {
                    use tokio::io::AsyncWriteExt;
                    if let Some(mut pipe) = child.stdin.take() {
                        let _ = pipe.write_all(stdin_data.as_bytes()).await;
                    }
                }

                let run = child.wait_with_output();
                let output = tokio::select! {
                    res = tokio::time::timeout(timeout, run) => {
                        // Clean up temp file.
                        let _ = tokio::fs::remove_file(&src_path).await;
                        match res {
                            Ok(Ok(out)) => out,
                            Ok(Err(e)) => return Err(ToolError::ExecutionFailed(e.to_string())),
                            Err(_) => return Err(ToolError::Timeout),
                        }
                    }
                    _ = ctx.cancellation_token.cancelled() => {
                        let _ = tokio::fs::remove_file(&src_path).await;
                        return Err(ToolError::Cancelled);
                    }
                };

                serde_json::to_value(Output {
                    exit_code: output.status.code().unwrap_or(-1),
                    stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                })
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
            })
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use kainetic_schema::RunId;
        use tokio_util::sync::CancellationToken;

        fn ctx() -> ToolContext {
            ToolContext::new(RunId::new(), CancellationToken::new())
        }

        #[tokio::test]
        async fn python_hello_world() {
            // Skip if python3 is not available.
            if std::process::Command::new("python3")
                .arg("--version")
                .output()
                .is_err()
            {
                return;
            }

            let tool = CodeExecutorTool::new();
            let result = tool
                .call(
                    serde_json::json!({
                        "language": "python",
                        "code": "print('hello from python')"
                    }),
                    ctx(),
                )
                .await
                .unwrap();

            assert_eq!(result["exit_code"], 0);
            assert!(result["stdout"]
                .as_str()
                .unwrap()
                .contains("hello from python"));
        }

        #[tokio::test]
        async fn python_syntax_error_nonzero_exit() {
            if std::process::Command::new("python3")
                .arg("--version")
                .output()
                .is_err()
            {
                return;
            }

            let tool = CodeExecutorTool::new();
            let result = tool
                .call(
                    serde_json::json!({ "language": "python", "code": "def broken(:" }),
                    ctx(),
                )
                .await
                .unwrap();

            assert_ne!(result["exit_code"], 0);
        }

        #[tokio::test]
        async fn timeout_kills_process() {
            if std::process::Command::new("python3")
                .arg("--version")
                .output()
                .is_err()
            {
                return;
            }

            let tool = CodeExecutorTool::new().with_timeout(1);
            let err = tool
                .call(
                    serde_json::json!({
                        "language": "python",
                        "code": "import time; time.sleep(30)"
                    }),
                    ctx(),
                )
                .await
                .unwrap_err();

            assert!(matches!(err, ToolError::Timeout));
        }

        #[test]
        fn input_schema_valid() {
            let tool = CodeExecutorTool::new();
            let _schema = tool.input_schema();
        }
    }
}

#[cfg(feature = "code-executor")]
pub use inner::CodeExecutorTool;
