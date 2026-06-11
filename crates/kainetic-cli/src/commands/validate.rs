//! `kainetic validate` — check project config and detect common issues.

use std::path::{Path, PathBuf};

use clap::Args;
use comfy_table::{Cell, Color, Table};
use console::style;

use crate::error::CliError;

/// Arguments for the `validate` subcommand.
#[derive(Args, Debug)]
pub struct ValidateArgs {
    /// Project directory to validate (defaults to current directory).
    #[arg(long, default_value = ".")]
    pub dir: PathBuf,
}

/// A single validation finding.
struct Finding {
    pass: bool,
    check: &'static str,
    detail: String,
}

/// Runs `kainetic validate`.
///
/// # Errors
///
/// Returns [`CliError::InvalidConfig`] if any check fails.
pub async fn run(args: ValidateArgs) -> Result<(), CliError> {
    let findings = vec![
        check_cargo_toml(&args.dir),
        check_kainetic_toml(&args.dir),
        check_src_exists(&args.dir),
    ];

    print_findings(&findings);

    let failures: Vec<_> = findings.iter().filter(|f| !f.pass).collect();
    if failures.is_empty() {
        println!("{} All checks passed.", style("✔").green().bold());
        Ok(())
    } else {
        let msg = failures
            .iter()
            .map(|f| format!("{}: {}", f.check, f.detail))
            .collect::<Vec<_>>()
            .join("; ");
        Err(CliError::InvalidConfig(msg))
    }
}

fn check_cargo_toml(dir: &Path) -> Finding {
    let path = dir.join("Cargo.toml");
    if !path.exists() {
        return Finding {
            pass: false,
            check: "Cargo.toml",
            detail: format!("{} not found", path.display()),
        };
    }
    match std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| toml::from_str::<toml::Value>(&s).ok())
    {
        Some(_) => Finding {
            pass: true,
            check: "Cargo.toml",
            detail: "valid TOML".into(),
        },
        None => Finding {
            pass: false,
            check: "Cargo.toml",
            detail: "invalid TOML syntax".into(),
        },
    }
}

fn check_kainetic_toml(dir: &Path) -> Finding {
    let path = dir.join(".kainetic.toml");
    if !path.exists() {
        return Finding {
            pass: false,
            check: ".kainetic.toml",
            detail: "not found — run `kainetic init` to create it".into(),
        };
    }
    match std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| toml::from_str::<toml::Value>(&s).ok())
    {
        Some(_) => Finding {
            pass: true,
            check: ".kainetic.toml",
            detail: "valid TOML".into(),
        },
        None => Finding {
            pass: false,
            check: ".kainetic.toml",
            detail: "invalid TOML syntax".into(),
        },
    }
}

fn check_src_exists(dir: &Path) -> Finding {
    let src = dir.join("src");
    if src.is_dir() {
        Finding {
            pass: true,
            check: "src/",
            detail: "directory present".into(),
        }
    } else {
        Finding {
            pass: false,
            check: "src/",
            detail: format!("{} directory not found", src.display()),
        }
    }
}

fn print_findings(findings: &[Finding]) {
    let mut table = Table::new();
    table.set_header(["Check", "Status", "Detail"]);

    for f in findings {
        let (status_cell, icon) = if f.pass {
            (
                Cell::new("PASS").fg(Color::Green),
                style("✔").green().to_string(),
            )
        } else {
            (
                Cell::new("FAIL").fg(Color::Red),
                style("✘").red().to_string(),
            )
        };
        table.add_row([
            Cell::new(format!("{icon} {}", f.check)),
            status_cell,
            Cell::new(&f.detail),
        ]);
    }

    println!("{table}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn validate_passes_on_valid_project() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_path_buf();

        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        std::fs::write(dir.join(".kainetic.toml"), "[project]\nname = \"x\"\n").unwrap();
        std::fs::create_dir(dir.join("src")).unwrap();

        let args = ValidateArgs { dir };
        run(args).await.unwrap();
    }

    #[tokio::test]
    async fn validate_fails_on_missing_files() {
        let tmp = tempfile::tempdir().unwrap();
        let args = ValidateArgs {
            dir: tmp.path().to_path_buf(),
        };
        let err = run(args).await.unwrap_err();
        assert!(matches!(err, CliError::InvalidConfig(_)));
    }

    #[tokio::test]
    async fn validate_fails_on_bad_toml() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_path_buf();

        std::fs::write(dir.join("Cargo.toml"), "not valid { toml !!").unwrap();
        std::fs::write(dir.join(".kainetic.toml"), "[project]\nname=\"x\"\n").unwrap();
        std::fs::create_dir(dir.join("src")).unwrap();

        let args = ValidateArgs { dir };
        let err = run(args).await.unwrap_err();
        assert!(matches!(err, CliError::InvalidConfig(_)));
    }
}
