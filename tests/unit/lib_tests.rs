use super::*;

#[test]
fn plugin_command_is_admin_with_description_and_id_is_public() {
    use snb_core::command::{CommandHandler, CommandVisibility};

    // `__SnbCommand_<fn>` is the unit struct the `#[command]` macro generates for
    // each annotated function; it lives in this crate's root module, so the test
    // can construct it directly. This asserts the security-critical declaration:
    // `/plugin` must be admin-scoped, `/id` public, both with a description.
    let plugin = crate::__SnbCommand_plugin_command;
    assert_eq!(plugin.name(), "plugin");
    assert_eq!(plugin.visibility(), CommandVisibility::Admin);
    assert!(!plugin.description().is_empty());

    let id = crate::__SnbCommand_id_command;
    assert_eq!(id.name(), "id");
    assert_eq!(id.visibility(), CommandVisibility::Public);
    assert!(!id.description().is_empty());
}

#[test]
fn identity_pane_labels_values_in_an_escaped_pre_block() {
    // A source with HTML-significant characters proves dynamic values are escaped.
    let mut event = Event::message("tg<&>", "/id");
    let message = event.message.as_mut().unwrap();
    message.to = Some("chat-42".to_string());
    message.from = Some("user-7".to_string());
    message.is_admin = true;

    let pane = identity_pane(&event);

    assert!(pane.contains("<b>Identity</b>"));
    assert!(pane.contains("<pre>"));

    for label in [
        "source",
        "sender",
        "chat id",
        "user id",
        "message id",
        "reply to",
        "chat type",
        "admin",
    ] {
        assert!(pane.contains(label), "missing label: {label}");
    }

    // Known values flow through; admin renders as "yes".
    assert!(pane.contains("chat-42"));
    assert!(pane.contains("user-7"));
    assert!(pane.contains("yes"));

    // The special characters in `source` are escaped, never emitted raw.
    assert!(pane.contains("tg&lt;&amp;&gt;"));
    assert!(!pane.contains("tg<&>"));
}
