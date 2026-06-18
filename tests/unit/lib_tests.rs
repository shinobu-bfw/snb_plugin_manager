use super::*;

#[test]
fn auth_denies_when_message_is_missing() {
    let event = Event::command("stdin", "plugin", "list").with_sender("stdin");
    assert!(!is_admin_event(&event));
}

#[test]
fn auth_denies_non_admin_message() {
    let mut event = Event::message("telegram", "/plugin list");
    event.message.as_mut().unwrap().is_admin = false;
    assert!(!is_admin_event(&event));
}

#[test]
fn auth_allows_admin_message() {
    let mut event = Event::message("telegram", "/plugin list");
    event.message.as_mut().unwrap().is_admin = true;
    assert!(is_admin_event(&event));
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
