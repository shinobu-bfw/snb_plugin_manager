mod args;
mod commands;
mod discovery;
mod process;
mod reply;

use snb_core::command::CommandContext;
use snb_core::event::{Event, Message, TextFormat};
use snb_macros::{command, plugin};

pub(crate) const PLUGIN_NAME: &str = "plugin_manager";

#[command(name = "plugin", aliases = ["plugins", "pm"])]
fn plugin_command(ctx: &CommandContext) -> anyhow::Result<()> {
    if !is_admin_event(ctx.event) {
        return Ok(());
    }

    let result = commands::handle(ctx);
    if let Err(error) = &result {
        reply::reply(ctx, format!("plugin manager error: {error:#}"));
    }
    result
}

#[command(name = "id")]
fn id_command(ctx: &CommandContext) -> anyhow::Result<()> {
    reply::reply_formatted(ctx, identity_markdown(ctx.event), TextFormat::Markdown);
    Ok(())
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

#[plugin(name = "plugin_manager", version = "0.1.0", kind = Plugin)]
struct PluginManager;

#[cfg(test)]
#[path = "../tests/unit/lib_tests.rs"]
mod lib_tests;
