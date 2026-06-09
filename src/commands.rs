use std::path::Path;

use snb_core::command::CommandContext;
use snb_core::plugin::{PluginInfo, PluginType};

use crate::PLUGIN_NAME;
use crate::args;
use crate::discovery::{self, LocalPlugin, PluginIndex};
use crate::process;
use crate::reply::reply;

pub(crate) fn handle(ctx: &CommandContext) -> anyhow::Result<()> {
    let args = args::parse(ctx.args)?;
    let Some(command) = args.first().map(String::as_str) else {
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
            info_plugin(ctx, name)
        }
        "load" => {
            let Some(target) = args.get(1) else {
                reply(ctx, "usage: /plugin load <name|dir|library-path>");
                return Ok(());
            };
            load_plugin(ctx, target)
        }
        "unload" | "remove" => {
            let Some(name) = args.get(1) else {
                reply(ctx, "usage: /plugin unload <name>");
                return Ok(());
            };
            unload_plugin(ctx, name)
        }
        "reload" => {
            let Some(name) = args.get(1) else {
                reply(ctx, "usage: /plugin reload <name> [name|dir|library-path]");
                return Ok(());
            };
            reload_plugin(ctx, name, args.get(2).map(String::as_str))
        }
        "update" => {
            let Some(name) = args.get(1) else {
                reply(ctx, "usage: /plugin update <name|dir> [loaded-plugin-name]");
                return Ok(());
            };
            update_plugin(ctx, name, args.get(2).map(String::as_str))
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
    let index = PluginIndex::discover()?;
    if index.locals.is_empty() && index.loaded.is_empty() {
        reply(ctx, "no plugins found");
        return Ok(());
    }

    let loaded_count = index.loaded.len();
    let mut lines = vec![format!(
        "plugins ({} local, {} loaded)",
        index.locals.len(),
        loaded_count
    )];

    for local in &index.locals {
        lines.push(format_local_summary(&index, local));
    }

    let orphaned = index.loaded_without_local();
    if !orphaned.is_empty() {
        lines.push("loaded without local source".to_string());
        for info in orphaned {
            lines.push(format!("- {} loaded", format_plugin_summary(&info)));
        }
    }

    reply(ctx, lines.join("\n"));
    Ok(())
}

fn info_plugin(ctx: &CommandContext, name: &str) -> anyhow::Result<()> {
    let index = PluginIndex::discover()?;
    let loaded_name = index.resolve_loaded_name(name).ok();
    let local = index.resolve_local(name).ok().or_else(|| {
        loaded_name
            .as_deref()
            .and_then(|name| index.local_for_loaded(name))
    });
    let loaded = loaded_name
        .as_deref()
        .and_then(|name| index.loaded.iter().find(|info| info.name == name).cloned())
        .or_else(|| {
            local
                .as_ref()
                .and_then(|local| index.loaded_for_local(local))
        });

    if local.is_none() && loaded.is_none() {
        anyhow::bail!("plugin not found: {name}");
    }

    let mut lines = Vec::new();
    if let Some(info) = &loaded {
        lines.push(format!("loaded: {}", format_plugin_summary(info)));
    } else {
        lines.push("loaded: no".to_string());
    }

    if let Some(local) = &local {
        lines.push(format!("local name: {}", local.display_name()));
        lines.push(format!("dir: {}", local.relative_dir(&index.root)));
        lines.push(format!("manifest: {}", local.manifest_kind()));
        if let Some(package_name) = &local.package_name {
            lines.push(format!("package: {package_name}"));
        }
        if let Some(version) = &local.version {
            lines.push(format!("source version: {version}"));
        }
        if !local.declared_names.is_empty() {
            lines.push(format!(
                "declared names: {}",
                local.declared_names.join(", ")
            ));
        }
        if let Some(library_name) = &local.library_name {
            lines.push(format!(
                "library: {}",
                discovery::dynamic_library_file_name(library_name)
            ));
        }
        lines.push(format!(
            "release path: {}",
            local.release_library_path(&index.root).display()
        ));
        if let Some(path) = local.existing_library_path(&index.root) {
            lines.push(format!("loadable path: {}", path.display()));
        } else {
            lines.push("loadable path: missing; run /plugin update first".to_string());
        }
        if local.dir.join(".git").is_dir() {
            let revision =
                process::current_revision(&local.dir).unwrap_or_else(|_| "unknown".to_string());
            lines.push(format!("git revision: {revision}"));
        }
    }

    reply(ctx, lines.join("\n"));
    Ok(())
}

fn load_plugin(ctx: &CommandContext, target: &str) -> anyhow::Result<()> {
    let index = PluginIndex::discover()?;
    let path = index.resolve_load_target(target)?;
    let bot = snb_core::context::bot();
    bot.clone().load_plugin(&path)?;

    let loaded_name = newest_loaded_name(&index).unwrap_or_else(|| target.to_string());
    reply(ctx, format!("loaded {loaded_name} from {}", path.display()));
    Ok(())
}

fn unload_plugin(ctx: &CommandContext, name: &str) -> anyhow::Result<()> {
    let index = PluginIndex::discover()?;
    let loaded_name = index.resolve_loaded_name(name)?;
    if loaded_name == PLUGIN_NAME {
        reply(
            ctx,
            "refusing to unload plugin_manager from its own command",
        );
        return Ok(());
    }

    snb_core::context::bot()
        .clone()
        .unload_plugin(&loaded_name)?;
    reply(ctx, format!("unloaded plugin {loaded_name}"));
    Ok(())
}

fn reload_plugin(ctx: &CommandContext, name: &str, target: Option<&str>) -> anyhow::Result<()> {
    let index = PluginIndex::discover()?;
    let loaded_name = index.resolve_loaded_name(name)?;
    if loaded_name == PLUGIN_NAME {
        reply(
            ctx,
            "refusing to reload plugin_manager from its own command",
        );
        return Ok(());
    }

    let path = match target {
        Some(target) => index.resolve_load_target(target)?,
        None => index
            .local_for_loaded(&loaded_name)
            .and_then(|local| local.existing_library_path(&index.root))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "no local built library found for {loaded_name}; pass a library path"
                )
            })?,
    };

    let bot = snb_core::context::bot();
    bot.clone().unload_plugin(&loaded_name)?;
    bot.clone().load_plugin(&path)?;
    reply(
        ctx,
        format!("reloaded {loaded_name} from {}", path.display()),
    );
    Ok(())
}

fn update_plugin(
    ctx: &CommandContext,
    target: &str,
    loaded_plugin_name: Option<&str>,
) -> anyhow::Result<()> {
    let index = PluginIndex::discover()?;
    let local = index.resolve_local(target)?;
    let loaded_name = match loaded_plugin_name {
        Some(name) => Some(
            index
                .resolve_loaded_name(name)
                .unwrap_or_else(|_| name.to_string()),
        ),
        None => index.loaded_for_local(&local).map(|info| info.name),
    };

    if local.matches_query(PLUGIN_NAME) || loaded_name.as_deref() == Some(PLUGIN_NAME) {
        reply(
            ctx,
            "refusing to update plugin_manager from its own command",
        );
        return Ok(());
    }

    let pull_message = match process::git_pull(&local.dir)? {
        Some(_) => "git pull --ff-only ok".to_string(),
        None => "not a git repository; skipped git pull".to_string(),
    };

    let bot = snb_core::context::bot();
    let was_loaded = loaded_name
        .as_deref()
        .is_some_and(|name| bot.get_plugin(name).is_some());
    let restore_path = local.existing_library_path(&index.root);
    if let Some(name) = &loaded_name
        && was_loaded
    {
        bot.clone().unload_plugin(name)?;
    }

    let build_result = process::build_plugin(&index.root, &local.relative_dir(&index.root));
    if let Err(error) = build_result {
        if was_loaded {
            let restore_message = match restore_path {
                Some(path) if path.is_file() => match bot.clone().load_plugin(&path) {
                    Ok(()) => format!("restored previous library from {}", path.display()),
                    Err(restore_error) => {
                        format!("failed to restore previous library: {restore_error:#}")
                    }
                },
                _ => "previous library was not found; plugin remains unloaded".to_string(),
            };
            reply(
                ctx,
                format!(
                    "updated source but rebuild failed for {}: {error:#}\n{restore_message}",
                    local.display_name()
                ),
            );
            return Ok(());
        }
        return Err(error);
    }

    let library_path = local.release_library_path(&index.root);
    bot.clone().load_plugin(&library_path)?;
    let revision = if local.dir.join(".git").is_dir() {
        process::current_revision(&local.dir).unwrap_or_else(|_| "unknown".to_string())
    } else {
        "local".to_string()
    };

    reply(
        ctx,
        format!(
            "updated {} ({}) to {revision}\n{}\nloaded from {}",
            local.display_name(),
            local.relative_dir(&index.root),
            pull_message,
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
    let derived_dir = process::github_repo_dir_name(github_url)?;
    let plugin_dir_name = plugin_dir_name.unwrap_or(&derived_dir);
    discovery::validate_plugin_dir_name(plugin_dir_name)?;

    let index = PluginIndex::discover()?;
    std::fs::create_dir_all(&index.plugins_root)?;
    let plugin_dir = index.plugins_root.join(plugin_dir_name);
    if plugin_dir.exists() {
        anyhow::bail!(
            "plugin directory already exists: {}. Use /plugin update {} instead.",
            plugin_dir.display(),
            plugin_dir_name
        );
    }

    process::git_clone(&index.plugins_root, github_url, plugin_dir_name)?;

    let index = PluginIndex::discover()?;
    let local = index.resolve_local(plugin_dir_name)?;
    process::build_plugin(&index.root, &local.relative_dir(&index.root))?;
    let library_path = local.release_library_path(&index.root);
    snb_core::context::bot()
        .clone()
        .load_plugin(&library_path)?;

    let revision = process::current_revision(&local.dir).unwrap_or_else(|_| "unknown".to_string());
    reply(
        ctx,
        format!(
            "installed {} from {github_url} at {revision}, loaded from {}",
            local.display_name(),
            library_path.display()
        ),
    );
    Ok(())
}

fn newest_loaded_name(previous: &PluginIndex) -> Option<String> {
    let bot = snb_core::context::bot();
    let mut names = bot.list_plugins();
    names.sort();
    names
        .into_iter()
        .find(|name| !previous.loaded.iter().any(|info| info.name == *name))
}

fn format_local_summary(index: &PluginIndex, local: &LocalPlugin) -> String {
    let loaded = index.loaded_for_local(local);
    let status = if loaded.is_some() {
        "loaded"
    } else {
        "unloaded"
    };
    let display = loaded
        .as_ref()
        .map(|info| format_plugin_summary(info))
        .unwrap_or_else(|| local.display_name());
    let source_version = local
        .version
        .as_deref()
        .map(|version| format!(" v{version}"))
        .unwrap_or_default();
    let library = local
        .existing_library_path(&index.root)
        .map(|path| format!(" lib={}", display_path(&index.root, &path)))
        .unwrap_or_else(|| " lib=missing".to_string());

    format!(
        "- {display}{source_version} [{status}] dir={} source={}{}",
        local.relative_dir(&index.root),
        local.manifest_kind(),
        library
    )
}

fn format_plugin_summary(info: &PluginInfo) -> String {
    format!(
        "{} v{} [{}] abi {}",
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

fn display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn help_text() -> &'static str {
    "usage:
/plugin list
/plugin info <name>
/plugin load <name|dir|library-path>
/plugin unload <name>
/plugin reload <name> [name|dir|library-path]
/plugin install <github-url> [plugin-dir]
/plugin update <name|dir> [loaded-plugin-name]

Names can be runtime names, package names, directory names, or short names such as tg."
}
