use super::*;

#[test]
fn esc_escapes_all_html_significant_characters() {
    // Element-content chars (& < >) plus the attribute-only quotes (" ').
    assert_eq!(
        esc(r#"<a href="x">tom & 'jerry'</a>"#),
        "&lt;a href=&quot;x&quot;&gt;tom &amp; &#39;jerry&#39;&lt;/a&gt;"
    );
}

#[test]
fn esc_escapes_ampersand_first_without_double_escaping() {
    // `&` is replaced before the other entities, so an input that already looks
    // like an entity is escaped exactly once.
    assert_eq!(esc("a & b"), "a &amp; b");
    assert_eq!(esc("&lt;"), "&amp;lt;");
}
