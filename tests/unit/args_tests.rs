use super::*;

#[test]
fn parse_plain_args() {
    assert_eq!(
        parse("load target/debug/libsnb_example.so").unwrap(),
        vec!["load", "target/debug/libsnb_example.so"]
    );
}

#[test]
fn parse_quoted_path() {
    assert_eq!(
        parse("load \"target/debug/my plugin.dll\"").unwrap(),
        vec!["load", "target/debug/my plugin.dll"]
    );
}

#[test]
fn reject_unclosed_quote() {
    assert!(parse("load \"missing").is_err());
}
