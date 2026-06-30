//! `kainetic deploy` — package and deploy an agent to Kainetic Cloud.
//!
//! ## Workflow
//!
//! 1. Read `kainetic.toml` in the current directory.
//! 2. Exchange the `KAINETIC_API_KEY` env var for a short-lived JWT via
//!    `POST /v1/auth/token`.
//! 3. Register the agent (or update it if the version already exists) via
//!    `POST /v1/agents`.
//! 4. Print the deployment URL.

use std::io::Write as IoWrite;

use clap::Args;
use serde::{Deserialize, Serialize};

use crate::error::CliError;

/// Arguments for the `deploy` subcommand.
#[derive(Args, Debug)]
pub struct DeployArgs {
    /// Path to `kainetic.toml` (defaults to `./kainetic.toml`).
    #[arg(long, default_value = "kainetic.toml")]
    pub manifest: String,

    /// Kainetic Cloud base URL.
    ///
    /// Defaults to `KAINETIC_CLOUD_URL` env var, then `https://cloud.kainetic.dev`.
    #[arg(long)]
    pub cloud_url: Option<String>,

    /// Skip the confirmation prompt.
    #[arg(long, short = 'y')]
    pub yes: bool,
}

/// Minimal `kainetic.toml` manifest we need for deploy.
#[derive(Debug, Deserialize)]
struct KaineticManifest {
    agent: AgentManifest,
}

#[derive(Debug, Deserialize)]
struct AgentManifest {
    name: String,
    version: Option<String>,
    description: Option<String>,
}

/// Request body sent to `POST /v1/agents`.
#[derive(Debug, Serialize)]
struct RegisterAgentBody<'a> {
    name: &'a str,
    version: Option<&'a str>,
    description: Option<&'a str>,
}

/// Response from `POST /v1/auth/token`.
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
}

/// Response from `POST /v1/agents`.
#[derive(Debug, Deserialize)]
struct AgentResponse {
    id: String,
    name: String,
    version: String,
}

/// Runs `kainetic deploy`.
///
/// # Errors
///
/// - [`CliError::Config`] — manifest not found or invalid
/// - [`CliError::Internal`] — network or cloud API error
pub async fn run(args: DeployArgs) -> Result<(), CliError> {
    // ── 1. Read manifest ───────────────────────────────────────────────────
    let manifest_src = std::fs::read_to_string(&args.manifest)
        .map_err(|e| CliError::InvalidConfig(format!("could not read '{}': {e}", args.manifest)))?;

    let manifest: KaineticManifest = toml::from_str(&manifest_src)
        .map_err(|e| CliError::InvalidConfig(format!("invalid kainetic.toml: {e}")))?;

    let cloud_url = args
        .cloud_url
        .clone()
        .or_else(|| std::env::var("KAINETIC_CLOUD_URL").ok())
        .unwrap_or_else(|| "https://cloud.kainetic.dev".into());

    let api_key = std::env::var("KAINETIC_API_KEY").map_err(|_| {
        CliError::InvalidConfig("KAINETIC_API_KEY environment variable not set".into())
    })?;

    println!(
        "Deploying agent '{}' v{} to {}",
        manifest.agent.name,
        manifest.agent.version.as_deref().unwrap_or("0.1.0"),
        cloud_url,
    );

    if !args.yes {
        print!("Continue? [y/N] ");
        IoWrite::flush(&mut std::io::stdout()).map_err(|e| CliError::Internal(e.to_string()))?;

        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .map_err(|e| CliError::Internal(e.to_string()))?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    let client = reqwest::Client::new();

    // ── 2. Exchange API key for JWT ────────────────────────────────────────
    let token_resp = client
        .post(format!("{cloud_url}/v1/auth/token"))
        .json(&serde_json::json!({ "api_key": api_key }))
        .send()
        .await
        .map_err(|e| CliError::Internal(format!("auth request failed: {e}")))?;

    if !token_resp.status().is_success() {
        let status = token_resp.status();
        let body = token_resp.text().await.unwrap_or_default();
        return Err(CliError::Internal(format!(
            "authentication failed ({status}): {body}"
        )));
    }

    let token: TokenResponse = token_resp
        .json()
        .await
        .map_err(|e| CliError::Internal(format!("malformed token response: {e}")))?;

    // ── 3. Register / update agent ─────────────────────────────────────────
    let body = RegisterAgentBody {
        name: &manifest.agent.name,
        version: manifest.agent.version.as_deref(),
        description: manifest.agent.description.as_deref(),
    };

    let agent_resp = client
        .post(format!("{cloud_url}/v1/agents"))
        .bearer_auth(&token.access_token)
        .json(&body)
        .send()
        .await
        .map_err(|e| CliError::Internal(format!("agent registration failed: {e}")))?;

    if !agent_resp.status().is_success() {
        let status = agent_resp.status();
        let body = agent_resp.text().await.unwrap_or_default();
        return Err(CliError::Internal(format!(
            "agent registration failed ({status}): {body}"
        )));
    }

    let agent: AgentResponse = agent_resp
        .json()
        .await
        .map_err(|e| CliError::Internal(format!("malformed agent response: {e}")))?;

    // ── 4. Report success ──────────────────────────────────────────────────
    println!(
        "\nDeployment successful!\n  Agent:   {} v{}\n  ID:      {}\n  Dashboard: {}/agents/{}",
        agent.name, agent.version, agent.id, cloud_url, agent.id
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_parses_minimal() {
        let toml_str = r#"
[agent]
name = "my-agent"
"#;
        let m: KaineticManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(m.agent.name, "my-agent");
        assert!(m.agent.version.is_none());
    }

    #[test]
    fn manifest_parses_full() {
        let toml_str = r#"
[agent]
name = "my-agent"
version = "1.2.3"
description = "Does things"
"#;
        let m: KaineticManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(m.agent.version.as_deref(), Some("1.2.3"));
        assert_eq!(m.agent.description.as_deref(), Some("Does things"));
    }
}
