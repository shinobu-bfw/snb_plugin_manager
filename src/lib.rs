use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use snb_core::command::CommandContext;
use snb_core::event::{Event, Message};
use snb_core::plugin::{PluginInfo, PluginType};
use snb_macros::{command, plugin};

const PLUGIN_NAME: &str = "plugin_manager";
const CONFIG_FILE: &str = "config.toml";
const DEFAULT_CONFIG: &str = r#"# Shinobu plugin manager authorization.
#
# Management commands are denied unless one of these exact values matches the
# incoming command event. Use `/plugin whoami` to inspect the values for your
# adapter.
[auth]
user_ids = []
senders = []
sources = []
chat_ids = []
"#;

#[command(name = "plugin", aliases = ["plugins", "pm"])]
fn plugin_command(ctx: &CommandContext) -> anyhow::Result<()> {
    let result = handle_command(ctx);
    if let Err(error) = &result {
        reply(ctx, format!("plugin manager error: {error:#}"));
    }
    result
}

fn handle_command(ctx: &CommandContext) -> anyhow::Result<()> {
    let args = parse_args(ctx.args)?;
    let Some(command) = args.first().map(|arg| arg.as_str()) else {
        if !ensure_authorized(ctx)? {
            return Ok(());
        }
        return list_plugins(ctx);
    };

    match command {
        "help" | "-h" | "--help" => {
            reply(ctx, help_text());
            Ok(())
        }
        "whoami" | "auth" => {
            reply(ctx, whoami_text(ctx)?);
            Ok(())
        }
        "list" | "ls" => list_plugins(ctx),
        "info" => {
            if !ensure_authorized(ctx)? {
                return Ok(());
            }
            let Some(name) = args.get(1) else {
                reply(ctx, "usage: /plugin info <name>");
                return Ok(());
            };
            info_plugin(ctx, name);
            Ok(())
        }
        "load" => {
            if !ensure_authorized(ctx)? {
                return Ok(());
            }
            let Some(path) = args.get(1) else {
                reply(ctx, "usage: /plugin load <library-path>");
                return Ok(());
            };
            load_plugin(ctx, path)
        }
        "unload" | "remove" => {
            if !ensure_authorized(ctx)? {
                return Ok(());
            }
            let Some(name) = args.get(1) else {
                reply(ctx, "usage: /plugin unload <name>");
                return Ok(());
            };
            unload_plugin(ctx, name)
        }
        "reload" => {
            if !ensure_authorized(ctx)? {
                return Ok(());
            }
            let (Some(name), Some(path)) = (args.get(1), args.get(2)) else {
                reply(ctx, "usage: /plugin reload <name> <library-path>");
                return Ok(());
            };
            reload_plugin(ctx, name, path)
        }
        "update" => {
            if !ensure_authorized(ctx)? {
                return Ok(());
            }
            let Some(plugin_dir) = args.get(1) else {
                reply(
                    ctx,
                    "usage: /plugin update <plugin-dir> [loaded-plugin-name]",
                );
                return Ok(());
            };
            update_plugin(ctx, plugin_dir, args.get(2).map(String::as_str))
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
    if !ensure_authorized(ctx)? {
        return Ok(());
    }

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
    let path = Path::new(name);
    let mut components = path.components();
    let Some(std::path::Component::Normal(_)) = components.next() else {
        anyhow::bail!("plugin dir must be a directory name under plugins/");
    };
    if components.next().is_some() {
        anyhow::bail!("plugin dir must not contain path separators");
    }

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
    if let Some(message) = &ctx.event.message {
        response.message.as_mut().unwrap().to = message.to.clone();
        response.message.as_mut().unwrap().reply_to = message.id.clone();
    }
    if let Some(sender) = &ctx.event.sender {
        response.receiver = Some(sender.clone());
    }
    snb_core::context::bot().emit_event(response);
}

struct AuthConfig {
    user_ids: Vec<String>,
    senders: Vec<String>,
    sources: Vec<String>,
    chat_ids: Vec<String>,
}

impl AuthConfig {
    fn from_toml(input: &str) -> anyhow::Result<Self> {
        let table = input.parse::<toml::Table>()?;
        let auth = table
            .get("auth")
            .and_then(toml::Value::as_table)
            .ok_or_else(|| anyhow::anyhow!("{CONFIG_FILE} must contain an [auth] table"))?;

        Ok(Self {
            user_ids: string_list(auth, "user_ids")?,
            senders: string_list(auth, "senders")?,
            sources: string_list(auth, "sources")?,
            chat_ids: string_list(auth, "chat_ids")?,
        })
    }

    fn is_authorized(&self, event: &Event) -> bool {
        let message = event.message.as_ref();
        exact_match(&self.user_ids, message.and_then(|m| m.from.as_deref()))
            || exact_match(&self.senders, event.sender.as_deref())
            || exact_match(&self.sources, Some(event.source.as_str()))
            || exact_match(&self.chat_ids, message.and_then(|m| m.to.as_deref()))
    }
}

fn string_list(table: &toml::Table, key: &str) -> anyhow::Result<Vec<String>> {
    let Some(value) = table.get(key) else {
        return Ok(Vec::new());
    };
    let Some(items) = value.as_array() else {
        anyhow::bail!("[auth].{key} must be an array of strings");
    };

    let mut strings = Vec::with_capacity(items.len());
    for item in items {
        let Some(value) = item.as_str() else {
            anyhow::bail!("[auth].{key} must contain only strings");
        };
        strings.push(value.to_string());
    }
    Ok(strings)
}

fn exact_match(allowed: &[String], value: Option<&str>) -> bool {
    let Some(value) = value else {
        return false;
    };
    allowed.iter().any(|item| item == value)
}

fn ensure_authorized(ctx: &CommandContext) -> anyhow::Result<bool> {
    let (auth, created_default) = load_auth_config()?;
    if auth.is_authorized(ctx.event) {
        return Ok(true);
    }

    let mut text = String::from("not authorized for plugin management");
    if created_default {
        text.push_str("\ncreated default config: configs/plugin_manager/config.toml");
    }
    text.push_str("\n\ncurrent identity:\n");
    text.push_str(&identity_text(ctx.event));
    text.push_str("\n\nAdd an exact value to [auth].user_ids, senders, sources, or chat_ids.");
    reply(ctx, text);
    Ok(false)
}

fn load_auth_config() -> anyhow::Result<(AuthConfig, bool)> {
    let helper = snb_core::context::PluginHelper::new(PLUGIN_NAME);
    match helper.load_config(Path::new(CONFIG_FILE)) {
        Ok(config) => Ok((AuthConfig::from_toml(&config)?, false)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            helper.write_config(Path::new(CONFIG_FILE), DEFAULT_CONFIG)?;
            Ok((AuthConfig::from_toml(DEFAULT_CONFIG)?, true))
        }
        Err(error) => Err(error.into()),
    }
}

fn whoami_text(ctx: &CommandContext) -> anyhow::Result<String> {
    let (auth, created_default) = load_auth_config()?;
    let authorized = auth.is_authorized(ctx.event);
    let mut text = format!(
        "authorized: {}\n{}",
        if authorized { "yes" } else { "no" },
        identity_text(ctx.event)
    );
    if created_default {
        text.push_str("\ncreated default config: configs/plugin_manager/config.toml");
    }
    Ok(text)
}

fn identity_text(event: &Event) -> String {
    let message = event.message.as_ref();
    format!(
        "source: {}\nsender: {}\nfrom: {}\nchat: {}",
        event.source,
        event.sender.as_deref().unwrap_or("-"),
        message_value(message, |message| message.from.as_deref()),
        message_value(message, |message| message.to.as_deref())
    )
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
/plugin whoami
/plugin list
/plugin info <name>
/plugin load <library-path>
/plugin unload <name>
/plugin reload <name> <library-path>
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
    fn auth_config_reads_allow_lists() {
        let config = AuthConfig::from_toml(
            r#"[auth]
user_ids = ["u1"]
senders = ["stdin"]
sources = ["telegram"]
chat_ids = ["c1"]
"#,
        )
        .unwrap();

        assert_eq!(config.user_ids, vec!["u1"]);
        assert_eq!(config.senders, vec!["stdin"]);
        assert_eq!(config.sources, vec!["telegram"]);
        assert_eq!(config.chat_ids, vec!["c1"]);
    }

    #[test]
    fn auth_config_denies_by_default() {
        let config = AuthConfig::from_toml(DEFAULT_CONFIG).unwrap();
        let event = Event::command("stdin", "plugin", "list").with_sender("stdin");
        assert!(!config.is_authorized(&event));
    }

    #[test]
    fn auth_config_allows_matching_user() {
        let config = AuthConfig::from_toml(
            r#"[auth]
user_ids = ["admin"]
"#,
        )
        .unwrap();
        let mut event = Event::message("telegram", "/plugin list");
        event.message.as_mut().unwrap().from = Some("admin".to_string());
        assert!(config.is_authorized(&event));
    }

    #[test]
    fn auth_config_allows_matching_sender() {
        let config = AuthConfig::from_toml(
            r#"[auth]
senders = ["stdin"]
"#,
        )
        .unwrap();
        let event = Event::command("stdin", "plugin", "list").with_sender("stdin");
        assert!(config.is_authorized(&event));
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
}
