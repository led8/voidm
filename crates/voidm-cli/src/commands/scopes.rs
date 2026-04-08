use anyhow::Result;
use clap::{Args, Subcommand};
use std::sync::Arc;
use voidm_core::{crud, db::Database};

#[derive(Subcommand)]
pub enum ScopesCommands {
    /// List all known scopes
    List,
    /// Auto-detect the current repo scope from the working directory
    Detect(DetectArgs),
}

#[derive(Args, Clone)]
pub struct DetectArgs {
    /// Emit `export VOIDM_SCOPE=<scope>` for shell eval instead of just printing the scope
    #[arg(long)]
    pub export: bool,
}

pub async fn run(cmd: ScopesCommands, db: &Arc<dyn Database>, json: bool) -> Result<()> {
    let pool = db.sqlite_pool().expect("SQLite backend required");
    match cmd {
        ScopesCommands::List => {
            let scopes = crud::list_scopes(pool).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&scopes)?);
            } else {
                if scopes.is_empty() {
                    println!("No scopes found.");
                } else {
                    for s in &scopes {
                        println!("{}", s);
                    }
                }
            }
        }
        ScopesCommands::Detect(_) => unreachable!("handled before DB init"),
    }
    Ok(())
}

/// Run `voidm scopes detect` — no DB required.
pub fn run_detect(args: DetectArgs, json: bool) -> Result<()> {
    let scope = detect_scope()?;

    if json {
        println!("{}", serde_json::json!({ "scope": scope }));
    } else if args.export {
        println!("export VOIDM_SCOPE={}", scope);
    } else {
        println!("{}", scope);
    }
    Ok(())
}

/// Walk up from $PWD to find the git root, then derive a normalized scope string.
/// Falls back to the current directory name if no git root is found.
fn detect_scope() -> Result<String> {
    let cwd = std::env::current_dir()?;

    // Walk up looking for .git
    let mut dir = cwd.as_path();
    loop {
        if dir.join(".git").exists() {
            let name = try_git_remote_name(dir).unwrap_or_else(|| {
                dir.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string()
            });
            return Ok(normalize_scope(&name));
        }
        match dir.parent() {
            Some(p) => dir = p,
            None => break,
        }
    }

    // Fallback: current directory name
    let name = cwd
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();
    Ok(normalize_scope(&name))
}

/// Try to extract repo name from `.git/config` remote origin URL.
fn try_git_remote_name(git_root: &std::path::Path) -> Option<String> {
    let config_path = git_root.join(".git").join("config");
    let content = std::fs::read_to_string(config_path).ok()?;

    // Find [remote "origin"] section and extract url =
    let mut in_origin = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == r#"[remote "origin"]"# {
            in_origin = true;
            continue;
        }
        if in_origin {
            if trimmed.starts_with('[') {
                break; // left the section
            }
            if let Some(rest) = trimmed.strip_prefix("url") {
                let url = rest
                    .trim_start_matches(|c: char| c == ' ' || c == '=')
                    .trim();
                return repo_name_from_url(url);
            }
        }
    }
    None
}

/// Extract repo name from a git URL.
/// e.g. `git@github.com:user/my-repo.git` → `my-repo`
///      `https://github.com/user/my-repo.git` → `my-repo`
fn repo_name_from_url(url: &str) -> Option<String> {
    let last = url.trim_end_matches('/').rsplit('/').next()?;
    let name = last.trim_end_matches(".git");
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Normalize a string to a clean scope: lowercase, replace non-alphanumeric with `-`, strip edges.
fn normalize_scope(s: &str) -> String {
    let normalized: String = s
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    normalized.trim_matches('-').to_string()
}
