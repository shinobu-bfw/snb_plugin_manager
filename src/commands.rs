use std::path::Path;

use snb_core::command::CommandContext;
use snb_core::plugin::{PluginInfo, PluginType};

use crate::PLUGIN_NAME;
use crate::args;
use crate::discovery::{self, PluginIndex};
use crate::process;
use crate::reply::{esc, pre_block, pre_kv, reply_html};

pub(crate) fn handle(ctx: &CommandContext) -> anyhow::Result<()> {
    let args = args::parse(ctx.args)?;
    let Some(command) = args.first().map(String::as_str) else {
        return list_plugins(ctx);
    };

    match command {
        "help" | "-h" | "--help" => {
            reply_html(ctx, help_text());
            Ok(())
        }
        "list" | "ls" => list_plugins(ctx),
        "info" => {
            let Some(name) = args.get(1) else {
                reply_html(ctx, "usage: <code>/plugin info &lt;name&gt;</code>");
                return Ok(());
            };
            info_plugin(ctx, name)
        }
        "load" => {
            let Some(target) = args.get(1) else {
                reply_html(ctx, "usage: <code>/plugin load &lt;name|dir|library-path&gt;</code>");
                return Ok(());
            };
            load_plugin(ctx, target)
        }
        "unload" | "remove" => {
            let Some(name) = args.get(1) else {
                reply_html(ctx, "usage: <code>/plugin unload &lt;name&gt;</code>");
                return Ok(());
            };
            unload_plugin(ctx, name)
        }
        "reload" => {
            let Some(name) = args.get(1) else {
                reply_html(
                    ctx,
                    "usage: <code>/plugin reload &lt;name&gt; [name|dir|library-path]</code>",
                );
                return Ok(());
            };
            reload_plugin(ctx, name, args.get(2).map(String::as_str))
        }
        "update" => {
            let Some(name) = args.get(1) else {
                reply_html(
                    ctx,
                    "usage: <code>/plugin update &lt;name|dir&gt; [loaded-plugin-name]</code>",
                );
                return Ok(());
            };
            update_plugin(ctx, name, args.get(2).map(String::as_str))
        }
        "install" => {
            let Some(github_url) = args.get(1) else {
                reply_html(
                    ctx,
                    "usage: <code>/plugin install &lt;github-url&gt; [plugin-dir]</code>",
                );
                return Ok(());
            };
            install_plugin(ctx, github_url, args.get(2).map(String::as_str))
        }
        other => {
            reply_html(
                ctx,
                format!(
                    "unknown plugin command: <code>{}</code>\n\n{}",
                    esc(other),
                    help_text()
                ),
            );
            Ok(())
        }
    }
}

fn list_plugins(ctx: &CommandContext) -> anyhow::Result<()> {
    let index = PluginIndex::discover()?;
    if index.locals.is_empty() && index.loaded.is_empty() {
        reply_html(ctx, "<i>no plugins found</i>");
        return Ok(());
    }

    // One aligned <pre> table (status · name · version · type · abi); the long
    // dir/lib paths go below into a collapsed, tap-to-expand blockquote.
    let mut rows: Vec<[String; 5]> = Vec::new();
    let mut paths = String::new();
    for local in &index.locals {
        let loaded = index.loaded_for_local(local);
        let status = if loaded.is_some() { "●" } else { "○" };
        let (name, version, kind, abi) = match &loaded {
            Some(info) => (
                info.name.clone(),
                format!("v{}", info.version),
                plugin_type_name(&info.plugin_type).to_string(),
                info.abi_version.to_string(),
            ),
            None => (
                local.display_name(),
                local
                    .version
                    .as_deref()
                    .map(|version| format!("v{version}"))
                    .unwrap_or_else(|| "-".to_string()),
                "-".to_string(),
                "-".to_string(),
            ),
        };
        rows.push([status.to_string(), name, version, kind, abi]);

        let lib = local
            .existing_library_path(&index.root)
            .map(|path| format!("<code>{}</code>", esc(display_path(&index.root, &path))))
            .unwrap_or_else(|| "<i>missing</i>".to_string());
        paths.push_str(&format!(
            "\n<b>{}</b>\n  dir <code>{}</code> · {}\n  lib {lib}",
            esc(local.display_name()),
            esc(local.relative_dir(&index.root)),
            esc(local.manifest_kind()),
        ));
    }

    let mut out = format!(
        "<b>📦 Plugins</b> — {} local · {} loaded\n{}",
        index.locals.len(),
        index.loaded.len(),
        render_table(&rows),
    );
    if !paths.is_empty() {
        out.push_str(&format!(
            "\n<blockquote expandable><b>paths</b>{paths}</blockquote>"
        ));
    }

    let orphaned = index.loaded_without_local();
    if !orphaned.is_empty() {
        out.push_str("\n\n<b>Loaded without local source</b>");
        for info in orphaned {
            out.push_str(&format!("\n● {}", format_plugin_summary(&info)));
        }
    }

    reply_html(ctx, out);
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

    let mut kv: Vec<(&str, String)> = Vec::new();
    match &loaded {
        Some(info) => {
            kv.push(("loaded", "yes".to_string()));
            kv.push((
                "summary",
                format!(
                    "v{} · {} · abi {}",
                    info.version,
                    plugin_type_name(&info.plugin_type),
                    info.abi_version
                ),
            ));
        }
        None => kv.push(("loaded", "no".to_string())),
    }

    if let Some(local) = &local {
        kv.push(("local name", local.display_name()));
        kv.push(("dir", local.relative_dir(&index.root)));
        kv.push(("manifest", local.manifest_kind().to_string()));
        if let Some(package_name) = &local.package_name {
            kv.push(("package", package_name.clone()));
        }
        if let Some(version) = &local.version {
            kv.push(("source ver", version.clone()));
        }
        if !local.declared_names.is_empty() {
            kv.push(("declared", local.declared_names.join(", ")));
        }
        if let Some(library_name) = &local.library_name {
            kv.push(("library", discovery::dynamic_library_file_name(library_name)));
        }
        kv.push((
            "release",
            local.release_library_path(&index.root).display().to_string(),
        ));
        match local.existing_library_path(&index.root) {
            Some(path) => kv.push(("loadable", path.display().to_string())),
            None => kv.push(("loadable", "missing; run /plugin update first".to_string())),
        }
        if local.dir.join(".git").is_dir() {
            kv.push((
                "git rev",
                process::current_revision(&local.dir).unwrap_or_else(|_| "unknown".to_string()),
            ));
        }
    }

    reply_html(ctx, format!("<b>{}</b>\n{}", esc(name), pre_kv(&kv)));
    Ok(())
}

fn load_plugin(ctx: &CommandContext, target: &str) -> anyhow::Result<()> {
    let index = PluginIndex::discover()?;
    let path = index.resolve_load_target(target)?;
    let bot = snb_core::context::bot();
    bot.clone().load_plugin(&path)?;

    let loaded_name = newest_loaded_name(&index).unwrap_or_else(|| target.to_string());
    reply_html(
        ctx,
        format!(
            "✅ loaded <b>{}</b> from <code>{}</code>",
            esc(loaded_name),
            esc(path.display().to_string())
        ),
    );
    Ok(())
}

fn unload_plugin(ctx: &CommandContext, name: &str) -> anyhow::Result<()> {
    let index = PluginIndex::discover()?;
    let loaded_name = index.resolve_loaded_name(name)?;
    if loaded_name == PLUGIN_NAME {
        reply_html(ctx, format!("⚠️ refusing to unload {PLUGIN_NAME} from its own command"));
        return Ok(());
    }

    snb_core::context::bot()
        .clone()
        .unload_plugin(&loaded_name)?;
    reply_html(ctx, format!("✅ unloaded plugin <b>{}</b>", esc(loaded_name)));
    Ok(())
}

fn reload_plugin(ctx: &CommandContext, name: &str, target: Option<&str>) -> anyhow::Result<()> {
    let index = PluginIndex::discover()?;
    let loaded_name = index.resolve_loaded_name(name)?;
    if loaded_name == PLUGIN_NAME {
        reply_html(ctx, format!("⚠️ refusing to reload {PLUGIN_NAME} from its own command"));
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
    reply_html(
        ctx,
        format!(
            "✅ reloaded <b>{}</b> from <code>{}</code>",
            esc(loaded_name),
            esc(path.display().to_string())
        ),
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
        reply_html(ctx, format!("⚠️ refusing to update {PLUGIN_NAME} from its own command"));
        return Ok(());
    }

    let pull_message = match process::git_pull(&local.dir)? {
        Some(_) => "<code>git pull --ff-only</code> ok".to_string(),
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
                    Ok(()) => format!("restored previous library from <code>{}</code>", esc(path.display().to_string())),
                    Err(restore_error) => {
                        format!("failed to restore previous library: {}", esc(format!("{restore_error:#}")))
                    }
                },
                _ => "previous library was not found; plugin remains unloaded".to_string(),
            };
            reply_html(
                ctx,
                format!(
                    "⚠️ updated source but rebuild failed for <b>{}</b>: {}\n{restore_message}",
                    esc(local.display_name()),
                    esc(format!("{error:#}"))
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

    reply_html(
        ctx,
        format!(
            "✅ updated <b>{}</b> (<code>{}</code>) to <code>{}</code>\n{}\nloaded from <code>{}</code>",
            esc(local.display_name()),
            esc(local.relative_dir(&index.root)),
            esc(revision),
            pull_message,
            esc(library_path.display().to_string())
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
    reply_html(
        ctx,
        format!(
            "✅ installed <b>{}</b> from <code>{}</code> at <code>{}</code>\nloaded from <code>{}</code>",
            esc(local.display_name()),
            esc(github_url),
            esc(revision),
            esc(library_path.display().to_string())
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

/// Render the plugin rows as one aligned monospace `<pre>` table. Columns are
/// padded on the raw value, then the whole table is HTML-escaped, so a stray
/// `&`/`<`/`>` in a name renders at the right width instead of breaking the grid.
fn render_table(rows: &[[String; 5]]) -> String {
    let headers = ["ST", "NAME", "VER", "TYPE", "ABI"];
    let mut width = [2usize, 4, 3, 4, 3];
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            width[i] = width[i].max(cell.chars().count());
        }
    }

    let header_row: [String; 5] = std::array::from_fn(|i| headers[i].to_string());
    let rule_row: [String; 5] = std::array::from_fn(|i| "-".repeat(width[i]));

    let mut body = pad_row(&header_row, &width);
    body.push('\n');
    body.push_str(&pad_row(&rule_row, &width));
    for row in rows {
        body.push('\n');
        body.push_str(&pad_row(row, &width));
    }

    pre_block(body)
}

fn pad_row(cells: &[String; 5], width: &[usize; 5]) -> String {
    (0..5)
        .map(|i| format!("{:<w$}", cells[i], w = width[i]))
        .collect::<Vec<_>>()
        .join("  ")
        .trim_end()
        .to_string()
}

fn format_plugin_summary(info: &PluginInfo) -> String {
    format!(
        "<b>{}</b> <i>v{}</i> · {} · abi {}",
        esc(&info.name),
        esc(info.version.to_string()),
        plugin_type_name(&info.plugin_type),
        esc(info.abi_version.to_string())
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

fn help_text() -> String {
    [
        "<b>Plugin manager</b>",
        "<code>/plugin list</code> — local &amp; loaded plugins",
        "<code>/plugin info &lt;name&gt;</code> — one plugin's details",
        "<code>/plugin load &lt;name|dir|library-path&gt;</code>",
        "<code>/plugin unload &lt;name&gt;</code>",
        "<code>/plugin reload &lt;name&gt; [name|dir|library-path]</code>",
        "<code>/plugin install &lt;github-url&gt; [plugin-dir]</code>",
        "<code>/plugin update &lt;name|dir&gt; [loaded-plugin-name]</code>",
        "",
        "<i>Names can be runtime names, package names, directory names, or short names such as tg.</i>",
    ]
    .join("\n")
}
