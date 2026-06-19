mod args;
mod commands;
mod discovery;
mod process;
mod reply;

use snb_core::command::CommandContext;
use snb_core::event::Event;
use snb_macros::{command, plugin};

pub(crate) const PLUGIN_NAME: &str = "PluginManager";

#[command(
    name = "plugin",
    aliases = ["plugins", "pm"],
    description = "Manage runtime plugins (admin only)",
    visibility = Admin
)]
fn plugin_command(ctx: &CommandContext) -> anyhow::Result<()> {
    let result = commands::handle(ctx);
    if let Err(error) = &result {
        reply::reply_html(
            ctx,
            format!("⚠️ <b>error:</b> {}", reply::esc(format!("{error:#}"))),
        );
    }
    result
}

#[command(name = "id", description = "Show chat & user identity")]
fn id_command(ctx: &CommandContext) -> anyhow::Result<()> {
    reply::reply_html(ctx, identity_pane(ctx.event));
    Ok(())
}

fn identity_pane(event: &Event) -> String {
    let message = event.message.as_ref();
    let sender = message.and_then(|m| m.sender.as_ref());
    let chat = message.map(|m| &m.chat);
    let opt = |value: Option<&str>| value.unwrap_or("-").to_string();
    let rows: Vec<(&str, String)> = vec![
        ("source", event.source.clone()),
        (
            "via",
            event.reply_plugin.clone().unwrap_or_else(|| "-".to_string()),
        ),
        ("chat id", opt(chat.map(|c| c.id.as_str()))),
        ("chat title", opt(chat.and_then(|c| c.title.as_deref()))),
        (
            "chat type",
            chat.and_then(|c| c.kind.as_ref())
                .map(chat_type_name)
                .unwrap_or("-")
                .to_string(),
        ),
        ("user id", opt(sender.map(|s| s.id.as_str()))),
        ("username", opt(sender.and_then(|s| s.username.as_deref()))),
        ("name", opt(sender.and_then(|s| s.display_name.as_deref()))),
        ("language", opt(sender.and_then(|s| s.language.as_deref()))),
        ("message id", opt(message.and_then(|m| m.id.as_deref()))),
        ("reply to", opt(message.and_then(|m| m.reply_to.as_deref()))),
        (
            "admin",
            if message.is_some_and(|m| m.is_admin) {
                "yes"
            } else {
                "no"
            }
            .to_string(),
        ),
    ];
    format!("<b>Identity</b>\n{}", reply::pre_kv(&rows))
}

fn chat_type_name(chat_type: &snb_core::event::ChatType) -> &'static str {
    match chat_type {
        snb_core::event::ChatType::Private => "private",
        snb_core::event::ChatType::Group => "group",
        snb_core::event::ChatType::Channel => "channel",
        snb_core::event::ChatType::Other(_) => "other",
    }
}

#[plugin(name = "PluginManager", version = "0.1.1", kind = Plugin)]
struct PluginManager;

#[cfg(test)]
#[path = "../tests/unit/lib_tests.rs"]
mod lib_tests;
