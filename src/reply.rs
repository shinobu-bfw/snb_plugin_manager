use snb_core::command::CommandContext;
use snb_core::event::{Event, TextFormat};

use crate::PLUGIN_NAME;

pub(crate) fn reply(ctx: &CommandContext, text: impl Into<String>) {
    let mut response = Event::message(PLUGIN_NAME, text.into());
    route_reply(ctx, &mut response);
    snb_core::context::bot().emit_event(response);
}

pub(crate) fn reply_formatted(ctx: &CommandContext, text: impl Into<String>, format: TextFormat) {
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
