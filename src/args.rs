pub(crate) fn parse(input: &str) -> anyhow::Result<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut quote = None;

    while let Some(ch) = chars.next() {
        match (ch, quote) {
            ('\\', Some(_)) => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            ('"' | '\'', None) => quote = Some(ch),
            (c, Some(q)) if c == q => quote = None,
            (c, None) if c.is_whitespace() => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            (c, _) => current.push(c),
        }
    }

    if let Some(q) = quote {
        anyhow::bail!("unterminated quote {q}");
    }
    if !current.is_empty() {
        args.push(current);
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
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
}
