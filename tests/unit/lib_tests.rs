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
    message.chat = snb_core::event::Chat {
        id: "chat-42".to_string(),
        kind: Some(snb_core::event::ChatType::Group),
        title: Some("My Group".to_string()),
        username: None,
        extra: Default::default(),
    };
    message.sender = Some(snb_core::event::Sender {
        id: "user-7".to_string(),
        username: Some("alice".to_string()),
        display_name: Some("Alice A".to_string()),
        first_name: Some("Alice".to_string()),
        last_name: Some("A".to_string()),
        is_bot: false,
        language: Some("en".to_string()),
        extra: Default::default(),
    });
    message.is_admin = true;

    let pane = identity_pane(&event);

    assert!(pane.contains("<b>Identity</b>"));
    assert!(pane.contains("<pre>"));

    for label in [
        "source",
        "via",
        "chat id",
        "chat title",
        "chat type",
        "user id",
        "username",
        "name",
        "language",
        "message id",
        "reply to",
        "admin",
    ] {
        assert!(pane.contains(label), "missing label: {label}");
    }

    // Known values flow through; admin renders as "yes".
    assert!(pane.contains("chat-42"));
    assert!(pane.contains("user-7"));
    assert!(pane.contains("yes"));
    assert!(pane.contains("alice"));
    assert!(pane.contains("Alice A"));
    assert!(pane.contains("My Group"));

    // The special characters in `source` are escaped, never emitted raw.
    assert!(pane.contains("tg&lt;&amp;&gt;"));
    assert!(!pane.contains("tg<&>"));
}
