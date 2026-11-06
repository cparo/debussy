//! A small JSON parser, enough to read `cargo metadata` (xtask builds on `std` and `anyhow` only:
//! docs/architecture/tooling.md "Repository tasks (xtask)").

use anyhow::{Result, bail};

/// A parsed JSON value. Numbers keep their source text; nothing here needs their value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Value>),
    /// Members in source order.
    Object(Vec<(String, Value)>),
}

impl Value {
    /// The member `key` of an object, or `None` if absent or not an object.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// The string, if this is one.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// The elements, if this is an array.
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(items) => Some(items),
            _ => None,
        }
    }
}

/// Parses a complete JSON document.
pub fn parse(text: &str) -> Result<Value> {
    let mut parser = Parser {
        bytes: text.as_bytes(),
        pos: 0,
    };
    let value = parser.value(0)?;
    parser.skip_whitespace();
    if parser.pos != parser.bytes.len() {
        bail!("trailing characters at byte {}", parser.pos);
    }
    Ok(value)
}

/// Deeper nesting is rejected rather than risking stack overflow.
const MAX_DEPTH: usize = 128;

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn value(&mut self, depth: usize) -> Result<Value> {
        if depth > MAX_DEPTH {
            bail!("nesting deeper than {MAX_DEPTH} at byte {}", self.pos);
        }
        self.skip_whitespace();
        match self.peek() {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => Ok(Value::String(self.string()?)),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'n') => self.literal("null", Value::Null),
            Some(b'-' | b'0'..=b'9') => Ok(self.number()),
            Some(other) => bail!("unexpected `{}` at byte {}", other as char, self.pos),
            None => bail!("unexpected end of input"),
        }
    }

    fn object(&mut self, depth: usize) -> Result<Value> {
        self.pos += 1;
        let mut members = Vec::new();
        self.skip_whitespace();
        if self.eat(b'}') {
            return Ok(Value::Object(members));
        }
        loop {
            self.skip_whitespace();
            if self.peek() != Some(b'"') {
                bail!("expected a member name at byte {}", self.pos);
            }
            let key = self.string()?;
            self.skip_whitespace();
            self.expect(b':')?;
            let value = self.value(depth + 1)?;
            members.push((key, value));
            self.skip_whitespace();
            if self.eat(b'}') {
                return Ok(Value::Object(members));
            }
            self.expect(b',')?;
        }
    }

    fn array(&mut self, depth: usize) -> Result<Value> {
        self.pos += 1;
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.eat(b']') {
            return Ok(Value::Array(items));
        }
        loop {
            items.push(self.value(depth + 1)?);
            self.skip_whitespace();
            if self.eat(b']') {
                return Ok(Value::Array(items));
            }
            self.expect(b',')?;
        }
    }

    fn string(&mut self) -> Result<String> {
        self.pos += 1;
        let mut out = String::new();
        loop {
            let start = self.pos;
            while let Some(b) = self.peek() {
                if b == b'"' || b == b'\\' || b < 0x20 {
                    break;
                }
                self.pos += 1;
            }
            // The input is a `&str` and the run stops only at ASCII bytes, so it is valid UTF-8.
            out.push_str(&String::from_utf8_lossy(&self.bytes[start..self.pos]));
            match self.peek() {
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    self.escape(&mut out)?;
                }
                Some(_) => bail!("control character in string at byte {}", self.pos),
                None => bail!("unterminated string"),
            }
        }
    }

    fn escape(&mut self, out: &mut String) -> Result<()> {
        let Some(b) = self.peek() else {
            bail!("unterminated escape");
        };
        self.pos += 1;
        let c = match b {
            b'"' => '"',
            b'\\' => '\\',
            b'/' => '/',
            b'b' => '\u{8}',
            b'f' => '\u{c}',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'u' => {
                let high = self.hex4()?;
                if (0xD800..0xDC00).contains(&high) {
                    if !(self.eat(b'\\') && self.eat(b'u')) {
                        bail!("unpaired surrogate at byte {}", self.pos);
                    }
                    let low = self.hex4()?;
                    if !(0xDC00..0xE000).contains(&low) {
                        bail!("invalid low surrogate at byte {}", self.pos);
                    }
                    let code = 0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00);
                    char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER)
                } else {
                    char::from_u32(high).unwrap_or(char::REPLACEMENT_CHARACTER)
                }
            }
            other => bail!("invalid escape `\\{}`", other as char),
        };
        out.push(c);
        Ok(())
    }

    fn hex4(&mut self) -> Result<u32> {
        let digits = self
            .bytes
            .get(self.pos..self.pos + 4)
            .and_then(|d| std::str::from_utf8(d).ok())
            .and_then(|d| u32::from_str_radix(d, 16).ok());
        let Some(value) = digits else {
            bail!("invalid \\u escape at byte {}", self.pos);
        };
        self.pos += 4;
        Ok(value)
    }

    fn number(&mut self) -> Value {
        let start = self.pos;
        while let Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') = self.peek() {
            self.pos += 1;
        }
        Value::Number(String::from_utf8_lossy(&self.bytes[start..self.pos]).into_owned())
    }

    fn literal(&mut self, word: &str, value: Value) -> Result<Value> {
        if self.bytes[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            Ok(value)
        } else {
            bail!("invalid literal at byte {}", self.pos)
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.peek() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn eat(&mut self, byte: u8) -> bool {
        let found = self.peek() == Some(byte);
        if found {
            self.pos += 1;
        }
        found
    }

    fn expect(&mut self, byte: u8) -> Result<()> {
        if !self.eat(byte) {
            bail!("expected `{}` at byte {}", byte as char, self.pos);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn test_parse_nested_document() -> Result<(), Box<dyn Error>> {
        let value = parse(r#" {"a": [1, -2.5e3, true, false, null], "b": {"c": "d"}, "e": []} "#)?;
        let a = value.get("a").and_then(Value::as_array).ok_or("no a")?;
        assert_eq!(a.len(), 5);
        assert_eq!(a[1], Value::Number("-2.5e3".into()));
        assert_eq!(a[4], Value::Null);
        let c = value
            .get("b")
            .and_then(|b| b.get("c"))
            .and_then(Value::as_str);
        assert_eq!(c, Some("d"));
        assert_eq!(value.get("missing"), None);
        Ok(())
    }

    #[test]
    fn test_parse_string_escapes() -> Result<(), Box<dyn Error>> {
        let value = parse(r#""q\"b\\s\/n\nt\tu\u00e9 \ud83c\udfb5 é""#)?;
        assert_eq!(value.as_str(), Some("q\"b\\s/n\nt\tué 🎵 é"));
        Ok(())
    }

    #[test]
    fn test_parse_rejects_malformed_input() {
        for bad in [
            "",
            "{",
            "[1,]",
            "{\"a\" 1}",
            "\"open",
            "tru",
            "1 2",
            "\"\\x\"",
            "\"\\ud83c\"",
            "{1: 2}",
            "\"a\nb\"",
        ] {
            assert!(parse(bad).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn test_parse_rejects_excessive_nesting() {
        let deep = "[".repeat(MAX_DEPTH + 2) + &"]".repeat(MAX_DEPTH + 2);
        assert!(parse(&deep).is_err());
        let ok = "[".repeat(MAX_DEPTH) + &"]".repeat(MAX_DEPTH);
        assert!(parse(&ok).is_ok());
    }
}
