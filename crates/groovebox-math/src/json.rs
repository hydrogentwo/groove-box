//! Zero-dependency JSON AST, parser, and Python-exact canonical serializer.
//!
//! `to_canonical_string()` matches `json.dumps(obj, sort_keys=True, separators=(",", ":"))`
//! character-for-character so SHA-256 composition/visual/spatial fingerprints
//! achieve bit-exact cross-language parity with the Python reference.

use std::collections::BTreeMap;
use crate::hash::format_python_f64;

#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Array(Vec<JsonValue>),
    Object(BTreeMap<String, JsonValue>),
}

impl JsonValue {
    pub fn obj() -> BTreeMap<String, JsonValue> {
        BTreeMap::new()
    }

    pub fn from_obj(map: BTreeMap<String, JsonValue>) -> Self {
        JsonValue::Object(map)
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            JsonValue::Float(v) => Some(*v),
            JsonValue::Int(v) => Some(*v as f64),
            JsonValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            JsonValue::Int(v) => Some(*v),
            JsonValue::Float(v) => Some(*v as i64),
            JsonValue::Bool(b) => Some(if *b { 1 } else { 0 }),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            JsonValue::Bool(b) => Some(*b),
            JsonValue::Int(v) => Some(*v != 0),
            JsonValue::Float(v) => Some(*v != 0.0),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            JsonValue::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[JsonValue]> {
        match self {
            JsonValue::Array(a) => Some(a.as_slice()),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&BTreeMap<String, JsonValue>> {
        match self {
            JsonValue::Object(m) => Some(m),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        self.as_object().and_then(|m| m.get(key))
    }

    /// Recursively round all floats to 12 decimal places (matches `visual_determinism._canon`).
    pub fn canonicalize_round12(&self) -> JsonValue {
        match self {
            JsonValue::Float(v) => {
                let factor = 1e12_f64;
                let r = (v * factor).round() / factor;
                JsonValue::Float(if r == 0.0 { 0.0 } else { r })
            }
            JsonValue::Array(arr) => {
                JsonValue::Array(arr.iter().map(|x| x.canonicalize_round12()).collect())
            }
            JsonValue::Object(map) => {
                let mut out = BTreeMap::new();
                for (k, v) in map {
                    out.insert(k.clone(), v.canonicalize_round12());
                }
                JsonValue::Object(out)
            }
            other => other.clone(),
        }
    }

    /// Compact canonical JSON string with sorted keys and `,` / `:` separators.
    pub fn to_canonical_string(&self) -> String {
        let mut out = String::new();
        self.write_compact(&mut out);
        out
    }

    pub fn to_pretty_string(&self) -> String {
        let mut out = String::new();
        self.write_pretty(&mut out, 0);
        out
    }

    fn write_compact(&self, out: &mut String) {
        match self {
            JsonValue::Null => out.push_str("null"),
            JsonValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            JsonValue::Int(i) => out.push_str(&i.to_string()),
            JsonValue::Float(f) => out.push_str(&format_python_f64(*f)),
            JsonValue::Str(s) => write_escaped_str(s, out),
            JsonValue::Array(arr) => {
                out.push('[');
                for (idx, v) in arr.iter().enumerate() {
                    if idx > 0 {
                        out.push(',');
                    }
                    v.write_compact(out);
                }
                out.push(']');
            }
            JsonValue::Object(map) => {
                out.push('{');
                for (idx, (k, v)) in map.iter().enumerate() {
                    if idx > 0 {
                        out.push(',');
                    }
                    write_escaped_str(k, out);
                    out.push(':');
                    v.write_compact(out);
                }
                out.push('}');
            }
        }
    }

    fn write_pretty(&self, out: &mut String, indent: usize) {
        match self {
            JsonValue::Array(arr) if !arr.is_empty() => {
                out.push_str("[\n");
                for (idx, v) in arr.iter().enumerate() {
                    for _ in 0..(indent + 2) {
                        out.push(' ');
                    }
                    v.write_pretty(out, indent + 2);
                    if idx + 1 < arr.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                for _ in 0..indent {
                    out.push(' ');
                }
                out.push(']');
            }
            JsonValue::Object(map) if !map.is_empty() => {
                out.push_str("{\n");
                for (idx, (k, v)) in map.iter().enumerate() {
                    for _ in 0..(indent + 2) {
                        out.push(' ');
                    }
                    write_escaped_str(k, out);
                    out.push_str(": ");
                    v.write_pretty(out, indent + 2);
                    if idx + 1 < map.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                for _ in 0..indent {
                    out.push(' ');
                }
                out.push('}');
            }
            _ => self.write_compact(out),
        }
    }
}

fn write_escaped_str(s: &str, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c if (c as u32) > 0x7f => {
                // Match Python's default ensure_ascii=True for exact canonical hash parity
                let code = c as u32;
                if code <= 0xFFFF {
                    out.push_str(&format!("\\u{:04x}", code));
                } else {
                    let v = code - 0x10000;
                    let hi = 0xD800 + (v >> 10);
                    let lo = 0xDC00 + (v & 0x3FF);
                    out.push_str(&format!("\\u{:04x}\\u{:04x}", hi, lo));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

impl From<bool> for JsonValue {
    fn from(v: bool) -> Self { JsonValue::Bool(v) }
}
impl From<i64> for JsonValue {
    fn from(v: i64) -> Self { JsonValue::Int(v) }
}
impl From<i32> for JsonValue {
    fn from(v: i32) -> Self { JsonValue::Int(v as i64) }
}
impl From<usize> for JsonValue {
    fn from(v: usize) -> Self { JsonValue::Int(v as i64) }
}
impl From<u32> for JsonValue {
    fn from(v: u32) -> Self { JsonValue::Int(v as i64) }
}
impl From<f64> for JsonValue {
    fn from(v: f64) -> Self { JsonValue::Float(v) }
}
impl From<f32> for JsonValue {
    fn from(v: f32) -> Self { JsonValue::Float(v as f64) }
}
impl From<&str> for JsonValue {
    fn from(v: &str) -> Self { JsonValue::Str(v.to_string()) }
}
impl From<String> for JsonValue {
    fn from(v: String) -> Self { JsonValue::Str(v) }
}

// =============================================================================
// Recursive-descent RFC 8259 JSON parser
// =============================================================================

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { bytes: input.as_bytes(), pos: 0 }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() {
            match self.bytes[self.pos] {
                b' ' | b'\t' | b'\n' | b'\r' => self.pos += 1,
                _ => break,
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn parse_value(&mut self) -> Result<JsonValue, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'n') => self.parse_lit(b"null", JsonValue::Null),
            Some(b't') => self.parse_lit(b"true", JsonValue::Bool(true)),
            Some(b'f') => self.parse_lit(b"false", JsonValue::Bool(false)),
            Some(b'"') => self.parse_string().map(JsonValue::Str),
            Some(b'[') => self.parse_array(),
            Some(b'{') => self.parse_object(),
            Some(b'-') | Some(b'0'..=b'9') => self.parse_number(),
            Some(c) => Err(format!("Unexpected byte '{}' at pos {}", c as char, self.pos)),
            None => Err("Unexpected end of JSON input".to_string()),
        }
    }

    fn parse_lit(&mut self, lit: &[u8], val: JsonValue) -> Result<JsonValue, String> {
        if self.bytes[self.pos..].starts_with(lit) {
            self.pos += lit.len();
            Ok(val)
        } else {
            Err(format!("Invalid literal at pos {}", self.pos))
        }
    }

    fn parse_string(&mut self) -> Result<String, String> {
        if self.peek() != Some(b'"') {
            return Err("Expected '\"'".to_string());
        }
        self.pos += 1;
        let mut out = String::new();
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b == b'"' {
                self.pos += 1;
                return Ok(out);
            }
            if b == b'\\' {
                self.pos += 1;
                if self.pos >= self.bytes.len() {
                    return Err("Unterminated escape".to_string());
                }
                let esc = self.bytes[self.pos];
                self.pos += 1;
                match esc {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\x08'),
                    b'f' => out.push('\x0c'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        if self.pos + 4 > self.bytes.len() {
                            return Err("Truncated \\u escape".to_string());
                        }
                        let hex = std::str::from_utf8(&self.bytes[self.pos..self.pos + 4])
                            .map_err(|_| "Invalid UTF-8 in \\u escape".to_string())?;
                        let code = u16::from_str_radix(hex, 16)
                            .map_err(|_| format!("Invalid hex in \\u{}", hex))?;
                        self.pos += 4;
                        if (0xD800..=0xDBFF).contains(&code)
                            && self.pos + 6 <= self.bytes.len()
                            && &self.bytes[self.pos..self.pos + 2] == b"\\u"
                        {
                            let hex2 = std::str::from_utf8(&self.bytes[self.pos + 2..self.pos + 6])
                                .map_err(|_| "Invalid UTF-8 in surrogate".to_string())?;
                            if let Ok(lo) = u16::from_str_radix(hex2, 16) {
                                if (0xDC00..=0xDFFF).contains(&lo) {
                                    self.pos += 6;
                                    let cp = 0x10000
                                        + (((code as u32 - 0xD800) << 10) | (lo as u32 - 0xDC00));
                                    if let Some(ch) = char::from_u32(cp) {
                                        out.push(ch);
                                        continue;
                                    }
                                }
                            }
                        }
                        if let Some(ch) = char::from_u32(code as u32) {
                            out.push(ch);
                        }
                    }
                    other => return Err(format!("Unknown escape \\{}", other as char)),
                }
            } else {
                // Read full UTF-8 char
                let rest = std::str::from_utf8(&self.bytes[self.pos..])
                    .map_err(|_| "Invalid UTF-8 in JSON string".to_string())?;
                let ch = rest.chars().next().ok_or("EOF in string")?;
                out.push(ch);
                self.pos += ch.len_utf8();
            }
        }
        Err("Unterminated JSON string".to_string())
    }

    fn parse_number(&mut self) -> Result<JsonValue, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        let mut is_float = false;
        if self.peek() == Some(b'.') {
            is_float = true;
            self.pos += 1;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            is_float = true;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.pos += 1;
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        let slice = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| "Invalid number UTF-8".to_string())?;
        if is_float {
            slice
                .parse::<f64>()
                .map(JsonValue::Float)
                .map_err(|e| format!("Invalid float '{}': {}", slice, e))
        } else if let Ok(i) = slice.parse::<i64>() {
            Ok(JsonValue::Int(i))
        } else {
            slice
                .parse::<f64>()
                .map(JsonValue::Float)
                .map_err(|e| format!("Invalid number '{}': {}", slice, e))
        }
    }

    fn parse_array(&mut self) -> Result<JsonValue, String> {
        self.pos += 1; // '['
        let mut arr = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(JsonValue::Array(arr));
        }
        loop {
            arr.push(self.parse_value()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(JsonValue::Array(arr));
                }
                _ => return Err(format!("Expected ',' or ']' at pos {}", self.pos)),
            }
        }
    }

    fn parse_object(&mut self) -> Result<JsonValue, String> {
        self.pos += 1; // '{'
        let mut map = BTreeMap::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(JsonValue::Object(map));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            if self.peek() != Some(b':') {
                return Err(format!("Expected ':' at pos {}", self.pos));
            }
            self.pos += 1;
            let val = self.parse_value()?;
            map.insert(key, val);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(JsonValue::Object(map));
                }
                _ => return Err(format!("Expected ',' or '}}' at pos {}", self.pos)),
            }
        }
    }
}

pub fn parse_json(input: &str) -> Result<JsonValue, String> {
    let mut p = Parser::new(input);
    let val = p.parse_value()?;
    p.skip_ws();
    if p.pos != p.bytes.len() {
        return Err(format!("Trailing characters at pos {}", p.pos));
    }
    Ok(val)
}
