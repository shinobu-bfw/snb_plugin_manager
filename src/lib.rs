mod args;
mod commands;
mod discovery;
mod process;
mod reply;

use snb_core::command::CommandContext;
use snb_core::event::{Event, Message};
use snb_macros::{command, plugin};

pub(crate) const PLUGIN_NAME: &str = "plugin_manager";

#[command(name = "plugin", aliases = ["plugins", "pm"])]
fn plugin_command(ctx: &CommandContext) -> anyhow::Result<()> {
    if !is_admin_event(ctx.event) {
        return Ok(());
    }

    let result = commands::handle(ctx);
    if let Err(error) = &result {
        reply::reply_html(
            ctx,
            format!("⚠️ <b>error:</b> {}", reply::esc(format!("{error:#}"))),
        );
    }
    result
}

#[command(name = "id")]
fn id_command(ctx: &CommandContext) -> anyhow::Result<()> {
    reply::reply_html(ctx, identity_pane(ctx.event));
    Ok(())
}

fn is_admin_event(event: &Event) -> bool {
    event
        .message
        .as_ref()
        .is_some_and(|message| message.is_admin)
}

fn identity_pane(event: &Event) -> String {
    let message = event.message.as_ref();
    let rows: Vec<(&str, String)> = vec![
        ("source", event.source.clone()),
        ("sender", event.sender.clone().unwrap_or_else(|| "-".to_string())),
        ("chat id", message_value(message, |message| message.to.as_deref()).to_string()),
        ("user id", message_value(message, |message| message.from.as_deref()).to_string()),
        ("message id", message_value(message, |message| message.id.as_deref()).to_string()),
        ("reply to", message_value(message, |message| message.reply_to.as_deref()).to_string()),
        (
            "chat type",
            message
                .and_then(|message| message.chat_type.as_ref())
                .map(chat_type_name)
                .unwrap_or("-")
                .to_string(),
        ),
        (
            "admin",
            if message.is_some_and(|message| message.is_admin) {
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

#[plugin(name = "plugin_manager", version = "0.1.0", kind = Plugin)]
struct PluginManager;

#[cfg(test)]
#[path = "../tests/unit/lib_tests.rs"]
mod lib_tests;
