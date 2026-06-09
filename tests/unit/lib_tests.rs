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
