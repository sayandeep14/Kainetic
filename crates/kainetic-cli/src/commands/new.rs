//! `kainetic new agent <name>` / `kainetic new tool <name>`.

use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use console::style;

use crate::error::CliError;

/// Arguments for the `new` subcommand.
#[derive(Args, Debug)]
pub struct NewArgs {
    #[command(subcommand)]
    pub kind: NewKind,
}

/// The type of artifact to generate.
#[derive(Subcommand, Debug)]
pub enum NewKind {
    /// Generate an agent skeleton file in `src/agents/`.
    Agent {
        /// Snake-case name for the new agent (e.g. `research_agent`).
        name: String,
    },
    /// Generate a tool skeleton file in `src/tools/`.
    Tool {
        /// Snake-case name for the new tool (e.g. `web_search`).
        name: String,
    },
}

/// Runs `kainetic new`.
///
/// # Errors
///
/// - [`CliError::AlreadyExists`] if the target file already exists.
/// - [`CliError::Io`] on filesystem errors.
pub async fn run(args: NewArgs) -> Result<(), CliError> {
    match args.kind {
        NewKind::Agent { name } => generate_agent(&name),
        NewKind::Tool { name } => generate_tool(&name),
    }
}

fn generate_agent(name: &str) -> Result<(), CliError> {
    let path = PathBuf::from(format!("src/agents/{name}.rs"));
    guard_exists(&path)?;
    std::fs::create_dir_all("src/agents")?;
    std::fs::write(&path, agent_template(name))?;
    success("agent", name, &path);
    Ok(())
}

fn generate_tool(name: &str) -> Result<(), CliError> {
    let path = PathBuf::from(format!("src/tools/{name}.rs"));
    guard_exists(&path)?;
    std::fs::create_dir_all("src/tools")?;
    std::fs::write(&path, tool_template(name))?;
    success("tool", name, &path);
    Ok(())
}

fn guard_exists(path: &Path) -> Result<(), CliError> {
    if path.exists() {
        return Err(CliError::AlreadyExists(path.display().to_string()));
    }
    Ok(())
}

fn success(kind: &str, name: &str, path: &Path) {
    println!(
        "{} Created {kind} `{}` at {}",
        style("✔").green().bold(),
        style(name).cyan(),
        style(path.display()).dim()
    );
    println!();
    println!("  Add `pub mod {name};` to your `src/{kind}s/mod.rs` to include it.");
}

// ── Templates ──────────────────────────────────────────────────────────────

fn agent_template(name: &str) -> String {
    let struct_name = pascal_case(name);
    format!(
        r#"//! {name} agent.
use kainetic::{{Agent, AgentConfig, AgentContext, AgentError, AgentFuture, ReActLoop}};

pub struct {struct_name}Agent {{
    config: AgentConfig,
}}

impl {struct_name}Agent {{
    pub fn new() -> Self {{
        Self {{
            config: AgentConfig::builder()
                .system_prompt("You are a helpful assistant.")
                .build(),
        }}
    }}
}}

impl Default for {struct_name}Agent {{
    fn default() -> Self {{ Self::new() }}
}}

impl Agent for {struct_name}Agent {{
    type Input = String;
    type Output = String;
    type Error = AgentError;

    fn name(&self) -> &'static str {{ "{name}" }}
    fn description(&self) -> &'static str {{ "TODO: describe this agent." }}
    fn config(&self) -> &AgentConfig {{ &self.config }}

    fn run(&self, input: String, ctx: AgentContext) -> AgentFuture<'_, String, AgentError> {{
        Box::pin(async move {{
            ReActLoop::new(self.config.clone()).execute(input, ctx).await
        }})
    }}
}}
"#
    )
}

fn tool_template(name: &str) -> String {
    let struct_name = pascal_case(name);
    format!(
        r#"//! {name} tool.
use kainetic::tools::{{Tool, ToolContext, ToolError, ToolFuture}};
use schemars::JsonSchema;
use serde::{{Deserialize, Serialize}};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct {struct_name}Input {{
    /// TODO: describe this field.
    pub query: String,
}}

#[derive(Debug, Serialize)]
pub struct {struct_name}Output {{
    pub result: String,
}}

pub struct {struct_name}Tool;

impl Tool for {struct_name}Tool {{
    type Input = {struct_name}Input;
    type Output = {struct_name}Output;
    type Error = ToolError;

    fn name(&self) -> &'static str {{ "{name}" }}
    fn description(&self) -> &'static str {{ "TODO: describe this tool." }}

    fn call(
        &self,
        input: {struct_name}Input,
        _ctx: ToolContext,
    ) -> ToolFuture<'_, {struct_name}Output, ToolError> {{
        Box::pin(async move {{
            // TODO: implement tool logic.
            Ok({struct_name}Output {{ result: input.query }})
        }})
    }}
}}
"#
    )
}

fn pascal_case(s: &str) -> String {
    s.split('_')
        .chain(s.split('-'))
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join("_")
        .split('_')
        .map(|part| {
            let mut c = part.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_template_contains_struct_name() {
        let t = agent_template("my_agent");
        assert!(t.contains("MyAgentAgent") || t.contains("struct"));
    }

    #[test]
    fn tool_template_contains_name() {
        let t = tool_template("web_search");
        assert!(t.contains("web_search"));
    }
}
