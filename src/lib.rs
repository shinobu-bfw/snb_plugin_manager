use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use snb_core::command::CommandContext;
use snb_core::event::{Event, Message, TextFormat};
use snb_core::plugin::{PluginInfo, PluginType};
use snb_macros::{command, plugin};

const PLUGIN_NAME: &str = "plugin_manager";

#[command(name = "plugin", aliases = ["plugins", "pm"])]
fn plugin_command(ctx: &CommandContext) -> anyhow::Result<()> {
    if !is_admin_event(ctx.event) {
        return Ok(());
    }

    let result = handle_command(ctx);
    if let Err(error) = &result {
        reply(ctx, format!("plugin manager error: {error:#}"));
    }
    result
}

#[command(name = "id")]
fn id_command(ctx: &CommandContext) -> anyhow::Result<()> {
    reply_formatted(ctx, identity_markdown(ctx.event), TextFormat::Markdown);
    Ok(())
}

fn handle_command(ctx: &CommandContext) -> anyhow::Result<()> {
    let args = parse_args(ctx.args)?;
    let Some(command) = args.first().map(|arg| arg.as_str()) else {
        return list_plugins(ctx);
    };

    match command {
        "help" | "-h" | "--help" => {
            reply(ctx, help_text());
            Ok(())
        }
        "list" | "ls" => list_plugins(ctx),
        "info" => {
            let Some(name) = args.get(1) else {
                reply(ctx, "usage: /plugin info <name>");
                return Ok(());
            };
            info_plugin(ctx, name);
            Ok(())
        }
        "load" => {
            let Some(path) = args.get(1) else {
                reply(ctx, "usage: /plugin load <library-path>");
                return Ok(());
            };
            load_plugin(ctx, path)
        }
        "unload" | "remove" => {
            let Some(name) = args.get(1) else {
                reply(ctx, "usage: /plugin unload <name>");
                return Ok(());
            };
            unload_plugin(ctx, name)
        }
        "reload" => {
            let (Some(name), Some(path)) = (args.get(1), args.get(2)) else {
                reply(ctx, "usage: /plugin reload <name> <library-path>");
                return Ok(());
            };
            reload_plugin(ctx, name, path)
        }
        "update" => {
            let Some(plugin_dir) = args.get(1) else {
                reply(
                    ctx,
                    "usage: /plugin update <plugin-dir> [loaded-plugin-name]",
                );
                return Ok(());
            };
            update_plugin(ctx, plugin_dir, args.get(2).map(String::as_str))
        }
        "install" => {
            let Some(github_url) = args.get(1) else {
                reply(ctx, "usage: /plugin install <github-url> [plugin-dir]");
                return Ok(());
            };
            install_plugin(ctx, github_url, args.get(2).map(String::as_str))
        }
        other => {
            reply(
                ctx,
                format!("unknown plugin command: {other}\n{}", help_text()),
            );
            Ok(())
        }
    }
}

fn list_plugins(ctx: &CommandContext) -> anyhow::Result<()> {
    let bot = snb_core::context::bot();
    let mut names = bot.list_plugins();
    names.sort();

    if names.is_empty() {
        reply(ctx, "no plugins loaded");
        return Ok(());
    }

    let mut lines = vec![format!("loaded plugins ({})", names.len())];
    for name in names {
        let Some(info) = bot.get_plugin(&name) else {
            continue;
        };
        lines.push(format_plugin_summary(&info));
    }
    reply(ctx, lines.join("\n"));
    Ok(())
}

fn info_plugin(ctx: &CommandContext, name: &str) {
    let bot = snb_core::context::bot();
    let Some(info) = bot.get_plugin(name) else {
        reply(ctx, format!("plugin not loaded: {name}"));
        return;
    };

    reply(
        ctx,
        format!(
            "plugin: {}\nversion: {}\ntype: {}\nabi: {}",
            info.name,
            info.version,
            plugin_type_name(&info.plugin_type),
            info.abi_version
        ),
    );
}

fn load_plugin(ctx: &CommandContext, path: &str) -> anyhow::Result<()> {
    let bot = snb_core::context::bot();
    let path = resolve_library_path(path);
    bot.clone().load_plugin(&path)?;
    reply(ctx, format!("loaded plugin from {}", path.display()));
    Ok(())
}

fn unload_plugin(ctx: &CommandContext, name: &str) -> anyhow::Result<()> {
    if name == PLUGIN_NAME {
        reply(
            ctx,
            "refusing to unload plugin_manager from its own command",
        );
        return Ok(());
    }

    let bot = snb_core::context::bot();
    bot.clone().unload_plugin(name)?;
    reply(ctx, format!("unloaded plugin {name}"));
    Ok(())
}

fn reload_plugin(ctx: &CommandContext, name: &str, path: &str) -> anyhow::Result<()> {
    if name == PLUGIN_NAME {
        reply(
            ctx,
            "refusing to reload plugin_manager from its own command",
        );
        return Ok(());
    }

    let bot = snb_core::context::bot();
    let path = resolve_library_path(path);
    bot.clone().unload_plugin(name)?;
    bot.clone().load_plugin(&path)?;
    reply(
        ctx,
        format!("reloaded plugin {name} from {}", path.display()),
    );
    Ok(())
}

fn update_plugin(
    ctx: &CommandContext,
    plugin_dir_name: &str,
    loaded_plugin_name: Option<&str>,
) -> anyhow::Result<()> {
    let plugin_dir = resolve_plugin_dir(plugin_dir_name)?;
    let loaded_plugin_name = loaded_plugin_name.unwrap_or(plugin_dir_name);

    if loaded_plugin_name == PLUGIN_NAME {
        reply(
            ctx,
            "refusing to update plugin_manager from its own command",
        );
        return Ok(());
    }

    run_command(
        Command::new("git")
            .arg("pull")
            .arg("--ff-only")
            .current_dir(&plugin_dir),
    )?;

    let bot = snb_core::context::bot();
    let was_loaded = bot.get_plugin(loaded_plugin_name).is_some();
    let library_path = release_library_path(&plugin_dir)?;
    if was_loaded {
        bot.clone().unload_plugin(loaded_plugin_name)?;
    }

    let build_result = run_command(
        Command::new("cargo")
            .arg("build")
            .arg("--release")
            .arg("--lib")
            .current_dir(&plugin_dir),
    );

    if let Err(error) = build_result {
        if was_loaded {
            let restore_result = if library_path.is_file() {
                bot.clone().load_plugin(&library_path)
            } else {
                Err(anyhow::anyhow!(
                    "previous library not found: {}",
                    library_path.display()
                ))
            };
            let restore_message = match restore_result {
                Ok(()) => "previous release library was loaded again".to_string(),
                Err(restore_error) => {
                    format!("failed to restore previous library: {restore_error:#}")
                }
            };
            reply(
                ctx,
                format!(
                    "updated source but rebuild failed after unloading {loaded_plugin_name}: {error:#}\n{restore_message}"
                ),
            );
            return Ok(());
        }
        return Err(error);
    }

    bot.clone().load_plugin(&library_path)?;

    let revision = current_revision(&plugin_dir).unwrap_or_else(|_| "unknown".to_string());
    reply(
        ctx,
        format!(
            "updated {plugin_dir_name} to {revision}, built release library, loaded from {}",
            library_path.display()
        ),
    );
    Ok(())
}

fn install_plugin(
    ctx: &CommandContext,
    github_url: &str,
    plugin_dir_name: Option<&str>,
) -> anyhow::Result<()> {
    let derived_dir = github_repo_dir_name(github_url)?;
    let plugin_dir_name = plugin_dir_name.unwrap_or(&derived_dir);
    validate_plugin_dir_name(plugin_dir_name)?;

    let plugins_root = std::env::current_dir()?.join("plugins");
    std::fs::create_dir_all(&plugins_root)?;
    let plugin_dir = plugins_root.join(plugin_dir_name);
    if plugin_dir.exists() {
        anyhow::bail!(
            "plugin directory already exists: {}. Use /plugin update {} instead.",
            plugin_dir.display(),
            plugin_dir_name
        );
    }

    run_command(
        Command::new("git")
            .arg("clone")
            .arg(github_url)
            .arg(plugin_dir_name)
            .current_dir(&plugins_root),
    )?;

    if !plugin_dir.join("Cargo.toml").is_file() {
        anyhow::bail!(
            "cloned repository does not contain Cargo.toml at {}",
            plugin_dir.display()
        );
    }

    run_command(
        Command::new("cargo")
            .arg("build")
            .arg("--release")
            .arg("--lib")
            .current_dir(&plugin_dir),
    )?;

    let library_path = release_library_path(&plugin_dir)?;
    snb_core::context::bot()
        .clone()
        .load_plugin(&library_path)?;

    let revision = current_revision(&plugin_dir).unwrap_or_else(|_| "unknown".to_string());
    reply(
        ctx,
        format!(
            "installed {plugin_dir_name} from {github_url} at {revision}, loaded from {}",
            library_path.display()
        ),
    );
    Ok(())
}

fn resolve_library_path(path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| Path::new(".").to_path_buf())
            .join(path)
    }
}

fn resolve_plugin_dir(name: &str) -> anyhow::Result<PathBuf> {
    validate_plugin_dir_name(name)?;

    let dir = std::env::current_dir()?.join("plugins").join(name);
    if !dir.is_dir() {
        anyhow::bail!("plugin directory not found: {}", dir.display());
    }
    if !dir.join(".git").is_dir() {
        anyhow::bail!(
            "plugin directory is not a git repository: {}",
            dir.display()
        );
    }
    if !dir.join("Cargo.toml").is_file() {
        anyhow::bail!("plugin Cargo.toml not found: {}", dir.display());
    }
    Ok(dir)
}

fn validate_plugin_dir_name(name: &str) -> anyhow::Result<()> {
    let path = Path::new(name);
    let mut components = path.components();
    let Some(std::path::Component::Normal(component)) = components.next() else {
        anyhow::bail!("plugin dir must be a directory name under plugins/");
    };
    if components.next().is_some() {
        anyhow::bail!("plugin dir must not contain path separators");
    }
    if component.to_string_lossy().starts_with('.') {
        anyhow::bail!("plugin dir must not start with '.'");
    }
    Ok(())
}

fn github_repo_dir_name(url: &str) -> anyhow::Result<String> {
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

fn release_library_path(plugin_dir: &Path) -> anyhow::Result<PathBuf> {
    let crate_name = cargo_library_name(&plugin_dir.join("Cargo.toml"))?;
    Ok(plugin_dir
        .join("target")
        .join("release")
        .join(dynamic_library_file_name(&crate_name)))
}

fn cargo_library_name(manifest: &Path) -> anyhow::Result<String> {
    let content = std::fs::read_to_string(manifest)?;
    let table = content.parse::<toml::Table>()?;
    if let Some(name) = table
        .get("lib")
        .and_then(toml::Value::as_table)
        .and_then(|lib| lib.get("name"))
        .and_then(toml::Value::as_str)
    {
        return Ok(name.replace('-', "_"));
    }

    let Some(package_name) = table
        .get("package")
        .and_then(toml::Value::as_table)
        .and_then(|package| package.get("name"))
        .and_then(toml::Value::as_str)
    else {
        anyhow::bail!("Cargo.toml must contain [package].name");
    };
    Ok(package_name.replace('-', "_"))
}

fn dynamic_library_file_name(crate_name: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{crate_name}.dll")
    } else if cfg!(target_os = "macos") {
        format!("lib{crate_name}.dylib")
    } else {
        format!("lib{crate_name}.so")
    }
}

fn current_revision(plugin_dir: &Path) -> anyhow::Result<String> {
    let output = run_command(
        Command::new("git")
            .arg("rev-parse")
            .arg("--short")
            .arg("HEAD")
            .current_dir(plugin_dir),
    )?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_command(command: &mut Command) -> anyhow::Result<Output> {
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

fn reply(ctx: &CommandContext, text: impl Into<String>) {
    let mut response = Event::message(PLUGIN_NAME, text.into());
    route_reply(ctx, &mut response);
    snb_core::context::bot().emit_event(response);
}

fn reply_formatted(ctx: &CommandContext, text: impl Into<String>, format: TextFormat) {
    let mut response = Event::formatted_message(PLUGIN_NAME, text.into(), format);
    route_reply(ctx, &mut response);
    snb_core::context::bot().emit_event(response);
}

fn route_reply(ctx: &CommandContext, response: &mut Event) {
    if let Some(message) = &ctx.event.message {
        response.message.as_mut().unwrap().to = message.to.clone();
        response.message.as_mut().unwrap().reply_to = message.id.clone();
    }
    if let Some(sender) = &ctx.event.sender {
        response.receiver = Some(sender.clone());
    }
}

fn is_admin_event(event: &Event) -> bool {
    event
        .message
        .as_ref()
        .is_some_and(|message| message.is_admin)
}

fn identity_markdown(event: &Event) -> String {
    let message = event.message.as_ref();
    format!(
        "*Identity*\nsource: `{}`\nsender: `{}`\nchat id: `{}`\nuser id: `{}`\nmessage id: `{}`\nreply to: `{}`\nchat type: `{}`\nadmin: `{}`",
        markdown_code(&event.source),
        markdown_code(event.sender.as_deref().unwrap_or("-")),
        markdown_code(message_value(message, |message| message.to.as_deref())),
        markdown_code(message_value(message, |message| message.from.as_deref())),
        markdown_code(message_value(message, |message| message.id.as_deref())),
        markdown_code(message_value(message, |message| message
            .reply_to
            .as_deref())),
        markdown_code(
            message
                .and_then(|message| message.chat_type.as_ref())
                .map(chat_type_name)
                .unwrap_or("-")
        ),
        message.is_some_and(|message| message.is_admin)
    )
}

fn markdown_code(value: &str) -> String {
    value.replace('\\', "\\\\").replace('`', "\\`")
}

fn chat_type_name(chat_type: &snb_core::event::ChatType) -> &'static str {
    match chat_type {
        snb_core::event::ChatType::Private => "private",
        snb_core::event::ChatType::Group => "group",
        snb_core::event::ChatType::Guild => "guild",
        snb_core::event::ChatType::Other(_) => "other",
    }
}

fn message_value<'a>(
    message: Option<&'a Message>,
    value: impl FnOnce(&'a Message) -> Option<&'a str>,
) -> &'a str {
    message.and_then(value).unwrap_or("-")
}

fn format_plugin_summary(info: &PluginInfo) -> String {
    format!(
        "- {} v{} [{}] abi {}",
        info.name,
        info.version,
        plugin_type_name(&info.plugin_type),
        info.abi_version
    )
}

fn plugin_type_name(plugin_type: &PluginType) -> &'static str {
    match plugin_type {
        PluginType::Adapter => "adapter",
        PluginType::Plugin => "plugin",
        PluginType::DatabaseDriver => "database",
    }
}

fn help_text() -> &'static str {
    "usage:
/plugin list
/plugin info <name>
/plugin load <library-path>
/plugin unload <name>
/plugin reload <name> <library-path>
/plugin install <github-url> [plugin-dir]
/plugin update <plugin-dir> [loaded-plugin-name]"
}

fn parse_args(input: &str) -> anyhow::Result<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut quote = None;

    while let Some(ch) = chars.next() {
        match (ch, quote) {
            ('\\', Some(_)) => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            ('"' | '\'', None) => quote = Some(ch),
            (c, Some(q)) if c == q => quote = None,
            (c, None) if c.is_whitespace() => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            (c, _) => current.push(c),
        }
    }

    if let Some(q) = quote {
        anyhow::bail!("unterminated quote {q}");
    }
    if !current.is_empty() {
        args.push(current);
    }
    Ok(args)
}

#[plugin(name = "plugin_manager", version = "0.1.0", kind = Plugin)]
struct PluginManager;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plain_args() {
        assert_eq!(
            parse_args("load target/debug/libsnb_example.so").unwrap(),
            vec!["load", "target/debug/libsnb_example.so"]
        );
    }

    #[test]
    fn parse_quoted_path() {
        assert_eq!(
            parse_args("load \"target/debug/my plugin.dll\"").unwrap(),
            vec!["load", "target/debug/my plugin.dll"]
        );
    }

    #[test]
    fn reject_unclosed_quote() {
        assert!(parse_args("load \"missing").is_err());
    }

    #[test]
    fn auth_denies_when_message_is_missing() {
        let event = Event::command("stdin", "plugin", "list").with_sender("stdin");
        assert!(!is_admin_event(&event));
    }

    #[test]
    fn auth_denies_non_admin_message() {
        let mut event = Event::message("telegram", "/plugin list");
        event.message.as_mut().unwrap().is_admin = false;
        assert!(!is_admin_event(&event));
    }

    #[test]
    fn auth_allows_admin_message() {
        let mut event = Event::message("telegram", "/plugin list");
        event.message.as_mut().unwrap().is_admin = true;
        assert!(is_admin_event(&event));
    }

    #[test]
    fn dynamic_library_name_uses_platform_convention() {
        let name = dynamic_library_file_name("snb_plugin_manager");
        if cfg!(target_os = "windows") {
            assert_eq!(name, "snb_plugin_manager.dll");
        } else if cfg!(target_os = "macos") {
            assert_eq!(name, "libsnb_plugin_manager.dylib");
        } else {
            assert_eq!(name, "libsnb_plugin_manager.so");
        }
    }

    #[test]
    fn cargo_library_name_reads_package_name() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        assert_eq!(cargo_library_name(&manifest).unwrap(), "snb_plugin_manager");
    }

    #[test]
    fn github_repo_dir_name_reads_https_url() {
        assert_eq!(
            github_repo_dir_name("https://github.com/owner/my-plugin.git").unwrap(),
            "my-plugin"
        );
    }

    #[test]
    fn github_repo_dir_name_reads_ssh_url() {
        assert_eq!(
            github_repo_dir_name("git@github.com:owner/my-plugin.git").unwrap(),
            "my-plugin"
        );
    }

    #[test]
    fn github_repo_dir_name_rejects_non_github_url() {
        assert!(github_repo_dir_name("https://example.com/owner/repo.git").is_err());
    }

    #[test]
    fn github_repo_dir_name_rejects_subpath_url() {
        assert!(github_repo_dir_name("https://github.com/owner/repo/tree/main").is_err());
    }
}
