use snb_core::command::CommandContext;
use snb_core::event::{Event, TextFormat};

use crate::PLUGIN_NAME;

pub(crate) fn reply_formatted(ctx: &CommandContext, text: impl Into<String>, format: TextFormat) {
    let mut response = Event::formatted_message(PLUGIN_NAME, text.into(), format);
    route_reply(ctx, &mut response);
    snb_core::context::bot().emit_event(response);
}

/// Send an HTML-formatted reply (Telegram supports `<b>`, `<i>`, `<code>`, `<a>`).
pub(crate) fn reply_html(ctx: &CommandContext, html: impl Into<String>) {
    reply_formatted(ctx, html, TextFormat::Html);
}

/// Escape a value for safe use in HTML text or a quoted attribute value.
/// `&` is escaped first so the entities below aren't double-escaped.
pub(crate) fn esc(value: impl AsRef<str>) -> String {
    value
        .as_ref()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Escape `raw` and wrap it in a plain `<pre>` monospace block. No `language-`
/// class is set, so Telegram applies no syntax highlighting.
pub(crate) fn pre_block(raw: impl AsRef<str>) -> String {
    format!("<pre>{}</pre>", esc(raw))
}

/// Render aligned `key   value` rows as a Telegram `<pre>` pane (keys are
/// right-padded to a common width). Values are escaped; keys are static.
pub(crate) fn pre_kv(rows: &[(&str, String)]) -> String {
    let key_width = rows.iter().map(|(key, _)| key.chars().count()).max().unwrap_or(0);
    let body = rows
        .iter()
        .map(|(key, value)| format!("{:<w$}  {}", key, value, w = key_width))
        .collect::<Vec<_>>()
        .join("\n");
    pre_block(body)
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

#[cfg(test)]
#[path = "../tests/unit/reply_tests.rs"]
mod reply_tests;
