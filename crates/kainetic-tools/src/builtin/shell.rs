//! [`ShellTool`] — executes shell commands in a subprocess.
//!
//! **Security note:** This tool runs arbitrary commands with the permissions
//! of the host process.  Only enable it in fully-trusted, sandboxed
//! environments.  Requires the `shell` crate feature.

#[cfg(feature = "shell")]
mod inner {
    use std::time::Duration;

    use kainetic_schema::RootSchema;
    use schemars::{schema_for, JsonSchema};
    use serde::{Deserialize, Serialize};
    use tracing::debug;

    use crate::{Tool, ToolContext, ToolError, ToolFuture};

    /// Executes a shell command and returns its stdout, stderr, and exit code.
    ///
    /// Commands time out after `timeout_secs` (default: 30).
    pub struct ShellTool {
        /// Working directory for spawned commands.  Defaults to current dir.
        working_dir: Option<std::path::PathBuf>,
        /// If set, only commands whose argv[0] matches an entry are allowed.
        allowed_commands: Option<Vec<String>>,
        /// Default timeout in seconds.
        timeout_secs: u64,
    }

    impl ShellTool {
        /// Creates a `ShellTool` with default settings (30 s timeout, no allowlist).
        #[must_use]
        pub fn new() -> Self {
            Self {
                working_dir: None,
                allowed_commands: None,
                timeout_secs: 30,
            }
        }

        /// Sets the working directory.
        #[must_use]
        pub fn with_working_dir(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
            self.working_dir = Some(dir.into());
            self
        }

        /// Restricts execution to commands in `allowed`.
        #[must_use]
        pub fn with_allowlist(mut self, allowed: Vec<String>) -> Self {
            self.allowed_commands = Some(allowed);
            self
        }

        /// Sets the default timeout.
        #[must_use]
        pub fn with_timeout(mut self, secs: u64) -> Self {
            self.timeout_secs = secs;
            self
        }
    }

    impl Default for ShellTool {
        fn default() -> Self {
            Self::new()
        }
    }

    #[derive(Deserialize, JsonSchema)]
    struct Input {
        /// Executable name (e.g. `"ls"`).
        command: String,
        /// Positional arguments.
        #[serde(default)]
        args: Vec<String>,
        /// Timeout in seconds (overrides instance default).
        timeout_secs: Option<u64>,
        /// Input to pipe into the process's stdin.
        stdin: Option<String>,
    }

    #[derive(Serialize, JsonSchema)]
    struct Output {
        exit_code: i32,
        stdout: String,
        stderr: String,
    }

    impl Tool for ShellTool {
        fn name(&self) -> &'static str {
            "shell"
        }

        fn description(&self) -> &'static str {
            "Execute a shell command and return its stdout, stderr, and exit code. \
             CAUTION: runs with host-process permissions."
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
                    .map_err(|e| ToolError::InputValidation(e.to_string()))?;

                if let Some(allowed) = &self.allowed_commands {
                    if !allowed.iter().any(|a| a == &params.command) {
                        return Err(ToolError::InputValidation(format!(
                            "command '{}' is not in the allowlist",
                            params.command
                        )));
                    }
                }

                debug!(command = %params.command, args = ?params.args, "shell");

                let timeout_secs =
                    Duration::from_secs(params.timeout_secs.unwrap_or(self.timeout_secs));

                let mut cmd = tokio::process::Command::new(&params.command);
                cmd.args(&params.args)
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped());

                if let Some(ref stdin_data) = params.stdin {
                    cmd.stdin(std::process::Stdio::piped());
                    let _ = stdin_data; // written below after spawn
                }

                if let Some(ref wd) = self.working_dir {
                    cmd.current_dir(wd);
                }

                let mut child = cmd
                    .spawn()
                    .map_err(|e| ToolError::ExecutionFailed(format!("spawn failed: {e}")))?;

                if let Some(stdin_data) = params.stdin {
                    use tokio::io::AsyncWriteExt;
                    if let Some(mut stdin_pipe) = child.stdin.take() {
                        let _ = stdin_pipe.write_all(stdin_data.as_bytes()).await;
                    }
                }

                let run = child.wait_with_output();
                let output = tokio::select! {
                    res = tokio::time::timeout(timeout_secs, run) => {
                        match res {
                            Ok(Ok(out)) => out,
                            Ok(Err(e)) => return Err(ToolError::ExecutionFailed(e.to_string())),
                            Err(_) => return Err(ToolError::Timeout),
                        }
                    }
                    _ = ctx.cancellation_token.cancelled() => {
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
        async fn echo_returns_text() {
            let tool = ShellTool::new();
            let result = tool
                .call(
                    serde_json::json!({ "command": "echo", "args": ["hello"] }),
                    ctx(),
                )
                .await
                .unwrap();
            assert_eq!(result["exit_code"], 0);
            assert!(result["stdout"].as_str().unwrap().contains("hello"));
        }

        #[tokio::test]
        async fn allowlist_blocks_unknown() {
            let tool = ShellTool::new().with_allowlist(vec!["echo".into()]);
            let err = tool
                .call(serde_json::json!({ "command": "ls", "args": [] }), ctx())
                .await
                .unwrap_err();
            assert!(matches!(err, ToolError::InputValidation(_)));
        }

        #[tokio::test]
        async fn timeout_fires() {
            let tool = ShellTool::new().with_timeout(1);
            let err = tool
                .call(
                    serde_json::json!({
                        "command": "sleep",
                        "args": ["10"],
                        "timeout_secs": 1
                    }),
                    ctx(),
                )
                .await
                .unwrap_err();
            assert!(matches!(err, ToolError::Timeout));
        }
    }
}

#[cfg(feature = "shell")]
pub use inner::ShellTool;
