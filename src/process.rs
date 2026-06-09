use std::path::Path;
use std::process::{Command, Output};

use crate::discovery::validate_plugin_dir_name;

pub(crate) fn git_pull(plugin_dir: &Path) -> anyhow::Result<Option<Output>> {
    if !plugin_dir.join(".git").is_dir() {
        return Ok(None);
    }

    run_command(
        Command::new("git")
            .arg("pull")
            .arg("--ff-only")
            .current_dir(plugin_dir),
    )
    .map(Some)
}

pub(crate) fn git_clone(
    plugins_root: &Path,
    github_url: &str,
    plugin_dir_name: &str,
) -> anyhow::Result<Output> {
    run_command(
        Command::new("git")
            .arg("clone")
            .arg(github_url)
            .arg(plugin_dir_name)
            .current_dir(plugins_root),
    )
}

pub(crate) fn build_plugin(root: &Path, plugin_ref: &str) -> anyhow::Result<Output> {
    run_command(
        Command::new("cargo")
            .arg("xtask")
            .arg("build-plugin")
            .arg(plugin_ref)
            .arg("--release")
            .current_dir(root),
    )
}

pub(crate) fn current_revision(plugin_dir: &Path) -> anyhow::Result<String> {
    let output = run_command(
        Command::new("git")
            .arg("rev-parse")
            .arg("--short")
            .arg("HEAD")
            .current_dir(plugin_dir),
    )?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub(crate) fn github_repo_dir_name(url: &str) -> anyhow::Result<String> {
    let trimmed = url
        .trim()
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .trim_end_matches('/');
    let path = if let Some(rest) = trimmed.strip_prefix("https://github.com/") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("http://github.com/") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("git@github.com:") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("ssh://git@github.com/") {
        rest
    } else {
        anyhow::bail!("install only accepts GitHub repository URLs");
    };

    let mut parts = path.split('/').filter(|part| !part.is_empty());
    let Some(owner) = parts.next() else {
        anyhow::bail!("GitHub URL is missing owner");
    };
    let Some(repo) = parts.next() else {
        anyhow::bail!("GitHub URL is missing repository");
    };
    if parts.next().is_some() {
        anyhow::bail!("GitHub URL must point to a repository root");
    }
    if owner.is_empty() {
        anyhow::bail!("GitHub URL is missing owner");
    }
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    validate_plugin_dir_name(repo)?;
    Ok(repo.to_string())
}

pub(crate) fn run_command(command: &mut Command) -> anyhow::Result<Output> {
    let output = command.output()?;
    if output.status.success() {
        return Ok(output);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::bail!(
        "command failed: {:?}\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        command,
        output.status,
        stdout.trim(),
        stderr.trim()
    );
}

#[cfg(test)]
#[path = "../tests/unit/process_tests.rs"]
mod process_tests;
