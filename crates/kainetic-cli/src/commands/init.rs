//! `kainetic init <name>` — scaffold a new Kainetic project.

use std::path::Path;

use clap::Args;
use console::style;

use crate::error::CliError;

/// Arguments for the `init` subcommand.
#[derive(Args, Debug)]
pub struct InitArgs {
    /// Name of the new project (also used as the directory name).
    pub name: String,

    /// Overwrite an existing directory if it already exists.
    #[arg(long)]
    pub force: bool,
}

/// Runs `kainetic init`.
///
/// # Errors
///
/// - [`CliError::AlreadyExists`] if the target directory exists and `--force`
///   was not passed.
/// - [`CliError::Io`] on filesystem errors.
pub async fn run(args: InitArgs) -> Result<(), CliError> {
    let dir = Path::new(&args.name);

    if dir.exists() && !args.force {
        return Err(CliError::AlreadyExists(format!(
            "directory `{}` already exists — pass --force to overwrite",
            args.name
        )));
    }

    println!(
        "{} Creating Kainetic project `{}`",
        style("✔").green().bold(),
        style(&args.name).cyan()
    );

    scaffold_project(dir, &args.name)?;

    println!();
    println!("{}", style("Project created!").green().bold());
    println!();
    println!("  Next steps:");
    println!(
        "    {} cd {}",
        style("1.").dim(),
        style(&args.name).cyan()
    );
    println!(
        "    {} export ANTHROPIC_API_KEY=sk-ant-...",
        style("2.").dim()
    );
    println!(
        "    {} kainetic run assistant --input '\"hello\"'",
        style("3.").dim()
    );

    Ok(())
}

fn scaffold_project(dir: &Path, name: &str) -> Result<(), CliError> {
    std::fs::create_dir_all(dir.join("src/agents"))?;
    std::fs::create_dir_all(dir.join("src/tools"))?;

    write_file(dir.join("Cargo.toml"), cargo_toml(name))?;
    write_file(dir.join("src/main.rs"), main_rs(name))?;
    write_file(dir.join("src/agents/assistant.rs"), agent_rs("assistant"))?;
    write_file(dir.join("src/tools/mod.rs"), "// Add custom tools here.\n")?;
    write_file(dir.join(".kainetic.toml"), kainetic_toml(name))?;
    write_file(dir.join(".gitignore"), "/target\n.env\n")?;
    write_file(dir.join(".env.example"), "ANTHROPIC_API_KEY=sk-ant-...\n")?;

    Ok(())
}

fn write_file(path: impl AsRef<Path>, content: impl AsRef<str>) -> Result<(), CliError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content.as_ref())?;
    println!("  {} {}", style("created").dim(), path.display());
    Ok(())
}

// ── Template strings ───────────────────────────────────────────────────────

fn cargo_toml(name: &str) -> String {
    format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "{name}"
path = "src/main.rs"

[dependencies]
kainetic = "0.1"          # uses the kainetic umbrella crate
tokio = {{ version = "1", features = ["full"] }}
tracing-subscriber = "0.3"
"#
    )
}

fn main_rs(name: &str) -> String {
    format!(
        r#"//! {name} — a Kainetic agentic application.
use kainetic::{{KaineticRuntime, providers::AnthropicProvider}};

mod agents;
mod tools;

#[tokio::main]
async fn main() {{
    tracing_subscriber::fmt().init();

    let provider = match AnthropicProvider::from_env() {{
        Ok(p) => p,
        Err(e) => {{
            eprintln!("ANTHROPIC_API_KEY not set: {{e}}");
            std::process::exit(1);
        }}
    }};

    let query = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Hello!".to_owned());

    let runtime = KaineticRuntime::builder()
        .provider(provider)
        .build();

    match runtime.run(&agents::assistant::AssistantAgent::new(), query).await {{
        Ok(reply) => println!("{{reply}}"),
        Err(e) => {{ eprintln!("{{e}}"); std::process::exit(1); }}
    }}
}}
"#
    )
}

fn agent_rs(name: &str) -> String {
    let struct_name = pascal_case(name);
    format!(
        r#"//! {name} agent.
use kainetic::{{Agent, AgentConfig, AgentContext, AgentError, AgentFuture, ReActLoop}};

/// A conversational assistant agent.
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
    fn description(&self) -> &'static str {{ "A helpful assistant." }}
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

fn kainetic_toml(name: &str) -> String {
    format!(
        r#"# .kainetic.toml — Kainetic project configuration
[project]
name = "{name}"
version = "0.1.0"

[agents]
# List agent binaries that `kainetic run` can launch.
# Each entry is the name passed to `--bin` when cargo-running.
default = "{name}"

[providers]
default = "anthropic"
"#
    )
}

/// Converts `my-agent` → `MyAgent`.
fn pascal_case(s: &str) -> String {
    s.split('-')
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
    fn pascal_case_single_word() {
        assert_eq!(pascal_case("assistant"), "Assistant");
    }

    #[test]
    fn pascal_case_hyphenated() {
        assert_eq!(pascal_case("my-cool-agent"), "MyCoolAgent");
    }

    #[tokio::test]
    async fn init_creates_expected_files() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("my-project");

        let args = InitArgs {
            name: dir.to_str().unwrap().to_owned(),
            force: false,
        };
        run(args).await.unwrap();

        assert!(dir.join("Cargo.toml").exists());
        assert!(dir.join("src/main.rs").exists());
        assert!(dir.join("src/agents/assistant.rs").exists());
        assert!(dir.join("src/tools/mod.rs").exists());
        assert!(dir.join(".kainetic.toml").exists());
        assert!(dir.join(".gitignore").exists());
    }

    #[tokio::test]
    async fn init_fails_if_exists_without_force() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("exists");
        std::fs::create_dir_all(&dir).unwrap();

        let args = InitArgs {
            name: dir.to_str().unwrap().to_owned(),
            force: false,
        };
        let err = run(args).await.unwrap_err();
        assert!(matches!(err, CliError::AlreadyExists(_)));
    }

    #[tokio::test]
    async fn init_with_force_overwrites() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("overwrite-me");
        std::fs::create_dir_all(&dir).unwrap();

        let args = InitArgs {
            name: dir.to_str().unwrap().to_owned(),
            force: true,
        };
        run(args).await.unwrap();
        assert!(dir.join("Cargo.toml").exists());
    }

    #[test]
    fn cargo_toml_contains_name() {
        let t = cargo_toml("myapp");
        assert!(t.contains("name = \"myapp\""));
    }

    #[test]
    fn kainetic_toml_is_valid_toml() {
        let t = kainetic_toml("myapp");
        toml::from_str::<toml::Value>(&t).expect("valid TOML");
    }
}
