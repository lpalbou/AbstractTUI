//! Hand-rolled JSON parser (no serde — dependency policy) sized for
//! glTF chunks: strict on the grammar, forgiving of unknown *fields*
//! (consumers look up what they know and ignore the rest).
//!
//! Design choices, and why:
//!
//! - **DOM, not streaming.** glTF JSON chunks are ≤ a few MB and the
//!   extractor does random-access lookups across sections (accessor →
//!   bufferView → buffer); a value tree is simpler to test exhaustively
//!   than a SAX callback web and the memory is bounded by the asset.
//! - **Objects are order-preserving `Vec<(String, Value)>`.** glTF
//!   objects are small (a handful of keys), so linear lookup beats a
//!   HashMap in both speed and code size. Duplicate keys keep the FIRST
//!   occurrence (lookup returns the first match; later dups are inert).
//! - **Numbers lex by the JSON grammar, convert via `f64::from_str`.**
//!   Writing a correct decimal→binary64 converter is its own project;
//!   std's is exactly rounded. The *grammar* (what is accepted) stays
//!   ours: leading zeros, `.5`, `1.`, `+1`, NaN/Infinity are rejected.
//! - **Depth limit.** Recursive descent + attacker-controlled nesting =
//!   stack exhaustion, and a panic (let alone a segfault) violates the
//!   no-panic quality bar. glTF nests ~6 levels; the limit is 128.

use crate::base::{Error, Result};

/// Maximum nesting depth (arrays + objects combined).
pub const MAX_DEPTH: usize = 128;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// Integral value if it is one exactly (glTF indices/counts must
    /// not be silently truncated from fractional numbers).
    pub fn as_usize(&self) -> Option<usize> {
        match self {
            Value::Number(n) if n.fract() == 0.0 && *n >= 0.0 && *n <= usize::MAX as f64 => {
                Some(*n as usize)
            }
            _ => None,
        }
    }

    pub fn as_u32(&self) -> Option<u32> {
        match self {
            Value::Number(n) if n.fract() == 0.0 && *n >= 0.0 && *n <= u32::MAX as f64 => {
                Some(*n as u32)
            }
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&[(String, Value)]> {
        match self {
            Value::Object(o) => Some(o),
            _ => None,
        }
    }

    /// Object member lookup (first match wins).
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(o) => o.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Array element lookup.
    pub fn idx(&self, i: usize) -> Option<&Value> {
        self.as_array().and_then(|a| a.get(i))
    }

    /// Iterate array elements (empty iterator for non-arrays —
    /// convenient for optional glTF list fields like `children`).
    pub fn elements(&self) -> std::slice::Iter<'_, Value> {
        match self {
            Value::Array(a) => a.iter(),
            _ => [].iter(),
        }
    }
}

/// Parse a complete JSON document (trailing whitespace allowed, any
/// other trailing bytes rejected).
pub fn parse(input: &str) -> Result<Value> {
    let mut p = Parser {
        bytes: input.as_bytes(),
        pos: 0,
        depth: 0,
    };
    p.skip_ws();
    let v = p.value()?;
    p.skip_ws();
    if p.pos != p.bytes.len() {
        return Err(p.err("trailing data after document"));
    }
    Ok(v)
}

/// Parse from raw bytes (the GLB JSON chunk); must be UTF-8.
pub fn parse_bytes(input: &[u8]) -> Result<Value> {
    let s = std::str::from_utf8(input)
        .map_err(|e| Error::Parse(format!("json: invalid utf-8 at byte {}", e.valid_up_to())))?;
    parse(s)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn err(&self, msg: &str) -> Error {
        Error::Parse(format!("json: {msg} at byte {}", self.pos))
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, b: u8) -> Result<()> {
        if self.peek() == Some(b) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.err(&format!("expected '{}'", b as char)))
        }
    }

    fn value(&mut self) -> Result<Value> {
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Value::String(self.string()?)),
            Some(b't') => self.literal(b"true", Value::Bool(true)),
            Some(b'f') => self.literal(b"false", Value::Bool(false)),
            Some(b'n') => self.literal(b"null", Value::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(c) => Err(self.err(&format!("unexpected byte 0x{c:02x}"))),
            None => Err(self.err("unexpected end of input")),
        }
    }

    fn literal(&mut self, word: &[u8], v: Value) -> Result<Value> {
        if self.bytes[self.pos..].starts_with(word) {
            self.pos += word.len();
            Ok(v)
        } else {
            Err(self.err("invalid literal"))
        }
    }

    fn object(&mut self) -> Result<Value> {
        self.enter()?;
        self.expect(b'{')?;
        let mut members = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            self.depth -= 1;
            return Ok(Value::Object(members));
        }
        loop {
            self.skip_ws();
            let key = self.string()?;
            self.skip_ws();
            self.expect(b':')?;
            self.skip_ws();
            let val = self.value()?;
            members.push((key, val));
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err(self.err("expected ',' or '}' in object")),
            }
        }
        self.depth -= 1;
        Ok(Value::Object(members))
    }

    fn array(&mut self) -> Result<Value> {
        self.enter()?;
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            self.depth -= 1;
            return Ok(Value::Array(items));
        }
        loop {
            self.skip_ws();
            items.push(self.value()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err(self.err("expected ',' or ']' in array")),
            }
        }
        self.depth -= 1;
        Ok(Value::Array(items))
    }

    fn enter(&mut self) -> Result<()> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            Err(self.err("nesting too deep"))
        } else {
            Ok(())
        }
    }

    fn string(&mut self) -> Result<String> {
        self.expect(b'"')?;
        let mut out = String::new();
        // `run_start` tracks the current run of literal (non-escape)
        // bytes; runs are flushed with a single slice copy, so a string
        // with no escapes costs exactly one allocation + one copy.
        let mut run_start = self.pos;
        loop {
            match self.peek() {
                None => return Err(self.err("unterminated string")),
                Some(b'"') => {
                    if run_start != self.pos {
                        out.push_str(self.str_slice(run_start, self.pos));
                    }
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    if run_start != self.pos {
                        out.push_str(self.str_slice(run_start, self.pos));
                    }
                    self.pos += 1;
                    let ch = self.escape()?;
                    out.push(ch);
                    run_start = self.pos;
                }
                Some(c) if c < 0x20 => return Err(self.err("raw control char in string")),
                Some(_) => self.pos += 1, // UTF-8 continuation bytes pass through
            }
        }
    }

    /// Decode one escape sequence (cursor sits just past the `\`).
    fn escape(&mut self) -> Result<char> {
        let esc = self.peek().ok_or_else(|| self.err("truncated escape"))?;
        self.pos += 1;
        Ok(match esc {
            b'"' => '"',
            b'\\' => '\\',
            b'/' => '/',
            b'b' => '\u{0008}',
            b'f' => '\u{000C}',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'u' => self.unicode_escape()?,
            _ => return Err(self.err("unknown escape")),
        })
    }

    /// `\uXXXX`, combining surrogate pairs; lone surrogates rejected
    /// (they are not scalar values and cannot enter a Rust String).
    fn unicode_escape(&mut self) -> Result<char> {
        let cp = self.hex4()?;
        if (0xD800..=0xDBFF).contains(&cp) {
            // High surrogate: exactly `\uXXXX` with a low surrogate
            // must follow.
            if self.peek() == Some(b'\\') && self.bytes.get(self.pos + 1) == Some(&b'u') {
                self.pos += 2;
                let lo = self.hex4()?;
                if !(0xDC00..=0xDFFF).contains(&lo) {
                    return Err(self.err("invalid low surrogate"));
                }
                let c = 0x10000 + (((cp - 0xD800) as u32) << 10 | (lo - 0xDC00) as u32);
                char::from_u32(c).ok_or_else(|| self.err("invalid surrogate pair"))
            } else {
                Err(self.err("lone high surrogate"))
            }
        } else if (0xDC00..=0xDFFF).contains(&cp) {
            Err(self.err("lone low surrogate"))
        } else {
            char::from_u32(cp as u32).ok_or_else(|| self.err("invalid \\u escape"))
        }
    }

    /// Slice `bytes[a..b]` as &str — safe because the parser only stops
    /// at ASCII delimiters, which never split a UTF-8 sequence.
    fn str_slice(&self, a: usize, b: usize) -> &'a str {
        std::str::from_utf8(&self.bytes[a..b]).expect("delimiters are ascii")
    }

    fn hex4(&mut self) -> Result<u16> {
        if self.bytes.len() - self.pos < 4 {
            return Err(self.err("truncated \\u escape"));
        }
        let mut v: u16 = 0;
        for _ in 0..4 {
            let b = self.bytes[self.pos];
            let d = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => return Err(self.err("bad hex digit in \\u escape")),
            };
            v = v << 4 | d as u16;
            self.pos += 1;
        }
        Ok(v)
    }

    fn number(&mut self) -> Result<Value> {
        let start = self.pos;
        // Lex strictly by RFC 8259 grammar; the validated slice is then
        // converted by std (exactly rounded).
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        match self.peek() {
            Some(b'0') => self.pos += 1, // a leading 0 is only itself
            Some(b'1'..=b'9') => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.pos += 1;
                }
            }
            _ => return Err(self.err("invalid number: no digits")),
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.err("invalid number: no digits after '.'"));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.err("invalid number: empty exponent"));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        let text = self.str_slice(start, self.pos);
        let n: f64 = text
            .parse()
            .map_err(|_| self.err("number out of representable range"))?;
        // Overflowing literals parse to ±inf in std; JSON has no inf.
        if !n.is_finite() {
            return Err(self.err("number overflows f64"));
        }
        Ok(Value::Number(n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Value {
        parse(s).unwrap()
    }

    #[test]
    fn scalars() {
        assert_eq!(p("null"), Value::Null);
        assert_eq!(p("true"), Value::Bool(true));
        assert_eq!(p("false"), Value::Bool(false));
        assert_eq!(p("42"), Value::Number(42.0));
        assert_eq!(p("-0.5"), Value::Number(-0.5));
        assert_eq!(p("2.5e3"), Value::Number(2500.0));
        assert_eq!(p("1E-2"), Value::Number(0.01));
        assert_eq!(p("-1.5e+2"), Value::Number(-150.0));
        assert_eq!(p("0"), Value::Number(0.0));
        assert_eq!(p("  \"hi\"\n"), Value::String("hi".into()));
    }

    #[test]
    fn number_grammar_strictness() {
        for bad in [
            "01", "+1", ".5", "1.", "1.e3", "1e", "1e+", "-", "NaN", "Infinity", "0x10",
        ] {
            assert!(parse(bad).is_err(), "{bad:?} should be rejected");
        }
        // Overflow to infinity is rejected, subnormal underflow is fine.
        assert!(parse("1e999").is_err());
        assert_eq!(p("1e-999"), Value::Number(0.0));
    }

    #[test]
    fn string_escapes() {
        assert_eq!(
            p(r#""a\"b\\c\/d\be\ff\ng\rh\ti""#).as_str().unwrap(),
            "a\"b\\c/d\u{8}e\u{c}f\ng\rh\ti"
        );
        assert_eq!(p(r#""\u0041\u00e9""#).as_str().unwrap(), "Aé");
        // Surrogate pair: U+1F600 (emoji) as \uD83D\uDE00.
        assert_eq!(p(r#""\uD83D\uDE00""#).as_str().unwrap(), "😀");
        // Raw UTF-8 passes through untouched.
        assert_eq!(p("\"héllo→🬻\"").as_str().unwrap(), "héllo→🬻");
    }

    #[test]
    fn string_errors() {
        for bad in [
            r#""\uD83D""#,       // lone high surrogate
            r#""\uDE00""#,       // lone low surrogate
            r#""\uD83D\u0041""#, // high surrogate + non-surrogate
            r#""\q""#,           // unknown escape
            r#""\u12g4""#,       // bad hex
            "\"abc",             // unterminated
            "\"a\nb\"",          // raw control char
        ] {
            assert!(parse(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn nesting_and_lookup() {
        let v = p(
            r#"{"meshes":[{"primitives":[{"attributes":{"POSITION":1},"mode":4}]}],"asset":{"version":"2.0"}}"#,
        );
        assert_eq!(
            v.get("asset")
                .and_then(|a| a.get("version"))
                .and_then(Value::as_str),
            Some("2.0")
        );
        let prim = v
            .get("meshes")
            .and_then(|m| m.idx(0))
            .and_then(|m| m.get("primitives"))
            .and_then(|ps| ps.idx(0))
            .unwrap();
        assert_eq!(prim.get("mode").and_then(Value::as_u32), Some(4));
        assert_eq!(
            prim.get("attributes")
                .and_then(|a| a.get("POSITION"))
                .and_then(Value::as_usize),
            Some(1)
        );
        assert!(prim.get("missing").is_none());
        assert_eq!(v.get("meshes").unwrap().elements().count(), 1);
    }

    #[test]
    fn as_usize_rejects_fractions_and_negatives() {
        assert_eq!(p("3.5").as_usize(), None);
        assert_eq!(p("-1").as_usize(), None);
        assert_eq!(p("3").as_usize(), Some(3));
        assert_eq!(p("4294967295").as_u32(), Some(u32::MAX));
        assert_eq!(p("4294967296").as_u32(), None);
    }

    #[test]
    fn duplicate_keys_keep_first() {
        let v = p(r#"{"a":1,"a":2}"#);
        assert_eq!(v.get("a").and_then(Value::as_f64), Some(1.0));
    }

    #[test]
    fn garbage_is_error_never_panic() {
        for bad in [
            "",
            "  ",
            "{",
            "}",
            "[",
            "]",
            "{]",
            "[}",
            "{\"a\"}",
            "{\"a\":}",
            "{\"a\":1,}",
            "[1,]",
            "[1 2]",
            "{'a':1}",
            "tru",
            "nul",
            "\u{0}",
            "{\"a\":1}x",
            "1 2",
            "[1],",
            "-.",
            "e5",
        ] {
            assert!(parse(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn depth_limit_enforced() {
        let deep_ok = format!("{}1{}", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH));
        assert!(parse(&deep_ok).is_ok());
        let too_deep = format!(
            "{}1{}",
            "[".repeat(MAX_DEPTH + 1),
            "]".repeat(MAX_DEPTH + 1)
        );
        let err = parse(&too_deep).unwrap_err();
        assert!(err.to_string().contains("deep"), "{err}");
    }

    #[test]
    fn parse_bytes_utf8_gate() {
        assert!(
            parse_bytes(b"{\"a\":1}   ").is_ok(),
            "trailing spaces = GLB padding"
        );
        assert!(parse_bytes(&[0x22, 0xFF, 0x22]).is_err(), "invalid utf-8");
    }
}
