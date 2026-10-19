//! Tiny JSON pretty-printer / minifier (no external crates, no full validation).

use std::iter::Peekable;
use std::str::Chars;

fn string(chars: &mut Peekable<Chars>, out: &mut String) -> Result<(), String> {
    out.push('"');
    loop {
        match chars.next() {
            None => return Err("unterminated string".into()),
            Some('\\') => {
                out.push('\\');
                if let Some(e) = chars.next() {
                    out.push(e);
                }
            }
            Some('"') => {
                out.push('"');
                return Ok(());
            }
            Some(c) => out.push(c),
        }
    }
}

fn skip_ws(chars: &mut Peekable<Chars>) {
    while chars.peek().is_some_and(|c| c.is_whitespace()) {
        chars.next();
    }
}

fn format(src: &str, unit: Option<&str>) -> Result<String, String> {
    let mut out = String::with_capacity(src.len() * 2);
    let mut stack: Vec<char> = Vec::new();
    let mut chars = src.chars().peekable();
    let newline = |out: &mut String, depth: usize| {
        if let Some(unit) = unit {
            out.push('\n');
            for _ in 0..depth {
                out.push_str(unit);
            }
        }
    };
    while let Some(c) = chars.next() {
        match c {
            '"' => string(&mut chars, &mut out)?,
            c if c.is_whitespace() => {}
            '{' | '[' => {
                let close = if c == '{' { '}' } else { ']' };
                skip_ws(&mut chars);
                out.push(c);
                if chars.peek() == Some(&close) {
                    chars.next();
                    out.push(close);
                } else {
                    stack.push(close);
                    newline(&mut out, stack.len());
                }
            }
            '}' | ']' => {
                if stack.pop() != Some(c) {
                    return Err(format!("unexpected '{c}'"));
                }
                newline(&mut out, stack.len());
                out.push(c);
            }
            ',' => {
                skip_ws(&mut chars);
                if matches!(chars.peek(), Some('}' | ']' | ',') | None) {
                    return Err("trailing comma".into());
                }
                out.push(',');
                newline(&mut out, stack.len());
            }
            ':' => out.push_str(if unit.is_some() { ": " } else { ":" }),
            '/' => return Err("comments are not supported".into()),
            c => out.push(c),
        }
    }
    match stack.pop() {
        Some(c) => Err(format!("missing '{c}'")),
        None => Ok(out),
    }
}

pub fn pretty(src: &str, unit: &str) -> Result<String, String> {
    format(src, Some(unit))
}

pub fn minify(src: &str) -> Result<String, String> {
    format(src, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let src = r#"{"a": [1, 2, {}], "b": {"c": "x, y: \"z\""}, "d": []}"#;
        let p = pretty(src, "  ").unwrap();
        assert_eq!(
            p,
            "{\n  \"a\": [\n    1,\n    2,\n    {}\n  ],\n  \"b\": {\n    \"c\": \"x, y: \\\"z\\\"\"\n  },\n  \"d\": []\n}"
        );
        assert_eq!(pretty(&p, "  ").unwrap(), p);
        assert_eq!(minify(&p).unwrap(), r#"{"a":[1,2,{}],"b":{"c":"x, y: \"z\""},"d":[]}"#);
        assert!(pretty("{\"a\": [1}", "  ").is_err());
        assert!(pretty("[1, 2", "  ").is_err());
        assert_eq!(pretty("[1, 2,]", "  ").unwrap_err(), "trailing comma");
        assert_eq!(minify("{\"a\": 1 , }").unwrap_err(), "trailing comma");
        assert!(pretty("[1,,2]", "  ").is_err());
    }
}
