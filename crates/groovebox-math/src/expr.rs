//! Safe, deterministic math expression & script evaluator for Seed scripts,
//! Domain equations, and Global Play Script/Domain algorithms.

use std::collections::HashMap;
use std::f64::consts::{E, PI, TAU};
use crate::constants::{MEUM, MEUM_INV, MEUM_MINUS_1, MEUM_NORM, PHI, PHI_INV};
use crate::operator_theory::{
    book_ics, book_ics_inv, book_isn, book_isn_inv, eqr_ics, eqr_isn, eqr_isn_inv,
    eqr_tensor_audio, ot_add, ot_div, ot_pow, ot_prod, ot_sub,
};

#[derive(Debug, Clone)]
pub struct EvalContext {
    pub vars: HashMap<String, f64>,
    pub ot_enabled: bool,
}

impl Default for EvalContext {
    fn default() -> Self {
        Self::new(0.0, 0.0, 0, 120.0, true)
    }
}

impl EvalContext {
    pub fn new(t: f64, seed: f64, index: usize, bpm: f64, ot_enabled: bool) -> Self {
        let mut vars = HashMap::new();
        vars.insert("MEUM".into(), MEUM);
        vars.insert("MEUM_NORM".into(), MEUM_NORM);
        vars.insert("MEUM_INV".into(), MEUM_INV);
        vars.insert("MEUM_MINUS_1".into(), MEUM_MINUS_1);
        vars.insert("PHI".into(), PHI);
        vars.insert("PHI_INV".into(), PHI_INV);
        vars.insert("PI".into(), PI);
        vars.insert("pi".into(), PI);
        vars.insert("TAU".into(), TAU);
        vars.insert("tau".into(), TAU);
        vars.insert("E".into(), E);
        vars.insert("e".into(), E);
        vars.insert("t".into(), t);
        vars.insert("time".into(), t);
        vars.insert("seed".into(), seed);
        vars.insert("i".into(), index as f64);
        vars.insert("index".into(), index as f64);
        vars.insert("bpm".into(), bpm);
        vars.insert("energy".into(), 0.5);
        vars.insert("phase".into(), (t * TAU).rem_euclid(TAU));
        Self { vars, ot_enabled }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Pow,
    LParen,
    RParen,
    Comma,
    EqEq,
    Neq,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Not,
    If,
    Else,
}

fn tokenize(src: &str) -> Result<Vec<Tok>, String> {
    let bytes = src.as_bytes();
    let mut pos = 0;
    let mut out = Vec::new();
    while pos < bytes.len() {
        let b = bytes[pos];
        if b.is_ascii_whitespace() {
            pos += 1;
            continue;
        }
        if b == b'#' {
            break;
        }
        if b.is_ascii_digit() || (b == b'.' && pos + 1 < bytes.len() && bytes[pos + 1].is_ascii_digit()) {
            let start = pos;
            while pos < bytes.len() && (bytes[pos].is_ascii_digit() || bytes[pos] == b'.') {
                pos += 1;
            }
            if pos < bytes.len() && matches!(bytes[pos], b'e' | b'E') {
                pos += 1;
                if pos < bytes.len() && matches!(bytes[pos], b'+' | b'-') {
                    pos += 1;
                }
                while pos < bytes.len() && bytes[pos].is_ascii_digit() {
                    pos += 1;
                }
            }
            let s = &src[start..pos];
            let n = s.parse::<f64>().map_err(|e| format!("Bad number '{}': {}", s, e))?;
            out.push(Tok::Num(n));
            continue;
        }
        if b.is_ascii_alphabetic() || b == b'_' {
            let start = pos;
            while pos < bytes.len() && (bytes[pos].is_ascii_alphanumeric() || bytes[pos] == b'_') {
                pos += 1;
            }
            let ident = &src[start..pos];
            match ident {
                "and" => out.push(Tok::And),
                "or" => out.push(Tok::Or),
                "not" => out.push(Tok::Not),
                "if" => out.push(Tok::If),
                "else" => out.push(Tok::Else),
                "True" | "true" => out.push(Tok::Num(1.0)),
                "False" | "false" => out.push(Tok::Num(0.0)),
                _ => out.push(Tok::Ident(ident.to_string())),
            }
            continue;
        }
        if pos + 1 < bytes.len() {
            let pair = &bytes[pos..pos + 2];
            let tok = match pair {
                b"**" => Some(Tok::Pow),
                b"==" => Some(Tok::EqEq),
                b"!=" => Some(Tok::Neq),
                b"<=" => Some(Tok::Le),
                b">=" => Some(Tok::Ge),
                _ => None,
            };
            if let Some(t) = tok {
                out.push(t);
                pos += 2;
                continue;
            }
        }
        match b {
            b'+' => out.push(Tok::Plus),
            b'-' => out.push(Tok::Minus),
            b'*' => out.push(Tok::Star),
            b'/' => out.push(Tok::Slash),
            b'%' => out.push(Tok::Percent),
            b'^' => out.push(Tok::Pow),
            b'(' => out.push(Tok::LParen),
            b')' => out.push(Tok::RParen),
            b',' => out.push(Tok::Comma),
            b'<' => out.push(Tok::Lt),
            b'>' => out.push(Tok::Gt),
            _ => return Err(format!("Unexpected char '{}' in expression", b as char)),
        }
        pos += 1;
    }
    Ok(out)
}

struct ExprParser<'a> {
    toks: &'a [Tok],
    pos: usize,
    ctx: &'a EvalContext,
}

impl<'a> ExprParser<'a> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn parse_ternary(&mut self) -> Result<f64, String> {
        let val = self.parse_or()?;
        if matches!(self.peek(), Some(Tok::If)) {
            self.pos += 1;
            let cond = self.parse_or()?;
            if !matches!(self.peek(), Some(Tok::Else)) {
                return Err("Expected 'else' in conditional expression".into());
            }
            self.pos += 1;
            let alt = self.parse_ternary()?;
            return Ok(if cond != 0.0 { val } else { alt });
        }
        Ok(val)
    }

    fn parse_or(&mut self) -> Result<f64, String> {
        let mut left = self.parse_and()?;
        while matches!(self.peek(), Some(Tok::Or)) {
            self.pos += 1;
            let right = self.parse_and()?;
            left = if left != 0.0 || right != 0.0 { 1.0 } else { 0.0 };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<f64, String> {
        let mut left = self.parse_cmp()?;
        while matches!(self.peek(), Some(Tok::And)) {
            self.pos += 1;
            let right = self.parse_cmp()?;
            left = if left != 0.0 && right != 0.0 { 1.0 } else { 0.0 };
        }
        Ok(left)
    }

    fn parse_cmp(&mut self) -> Result<f64, String> {
        let mut left = self.parse_add()?;
        while let Some(op) = self.peek().cloned() {
            match op {
                Tok::EqEq | Tok::Neq | Tok::Lt | Tok::Le | Tok::Gt | Tok::Ge => {
                    self.pos += 1;
                    let right = self.parse_add()?;
                    let b = match op {
                        Tok::EqEq => (left - right).abs() < 1e-12,
                        Tok::Neq => (left - right).abs() >= 1e-12,
                        Tok::Lt => left < right,
                        Tok::Le => left <= right,
                        Tok::Gt => left > right,
                        Tok::Ge => left >= right,
                        _ => false,
                    };
                    left = if b { 1.0 } else { 0.0 };
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_add(&mut self) -> Result<f64, String> {
        let mut left = self.parse_mul()?;
        while let Some(op) = self.peek().cloned() {
            match op {
                Tok::Plus => {
                    self.pos += 1;
                    left += self.parse_mul()?;
                }
                Tok::Minus => {
                    self.pos += 1;
                    left -= self.parse_mul()?;
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_mul(&mut self) -> Result<f64, String> {
        let mut left = self.parse_unary()?;
        while let Some(op) = self.peek().cloned() {
            match op {
                Tok::Star => {
                    self.pos += 1;
                    left *= self.parse_unary()?;
                }
                Tok::Slash => {
                    self.pos += 1;
                    let r = self.parse_unary()?;
                    left = if r.abs() < 1e-15 { 0.0 } else { left / r };
                }
                Tok::Percent => {
                    self.pos += 1;
                    let r = self.parse_unary()?;
                    left = if r.abs() < 1e-15 { 0.0 } else { left.rem_euclid(r) };
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<f64, String> {
        match self.peek() {
            Some(Tok::Plus) => {
                self.pos += 1;
                self.parse_unary()
            }
            Some(Tok::Minus) => {
                self.pos += 1;
                Ok(-self.parse_unary()?)
            }
            Some(Tok::Not) => {
                self.pos += 1;
                let v = self.parse_unary()?;
                Ok(if v == 0.0 { 1.0 } else { 0.0 })
            }
            _ => self.parse_pow(),
        }
    }

    fn parse_pow(&mut self) -> Result<f64, String> {
        let base = self.parse_primary()?;
        if matches!(self.peek(), Some(Tok::Pow)) {
            self.pos += 1;
            let exp = self.parse_unary()?;
            Ok(base.powf(exp))
        } else {
            Ok(base)
        }
    }

    fn parse_primary(&mut self) -> Result<f64, String> {
        match self.peek().cloned() {
            Some(Tok::Num(n)) => {
                self.pos += 1;
                Ok(n)
            }
            Some(Tok::LParen) => {
                self.pos += 1;
                let v = self.parse_ternary()?;
                if !matches!(self.peek(), Some(Tok::RParen)) {
                    return Err("Expected ')'".into());
                }
                self.pos += 1;
                Ok(v)
            }
            Some(Tok::Ident(name)) => {
                self.pos += 1;
                if matches!(self.peek(), Some(Tok::LParen)) {
                    self.pos += 1;
                    let mut args = Vec::new();
                    if !matches!(self.peek(), Some(Tok::RParen)) {
                        loop {
                            args.push(self.parse_ternary()?);
                            if matches!(self.peek(), Some(Tok::Comma)) {
                                self.pos += 1;
                            } else {
                                break;
                            }
                        }
                    }
                    if !matches!(self.peek(), Some(Tok::RParen)) {
                        return Err(format!("Expected ')' after call to {}", name));
                    }
                    self.pos += 1;
                    call_builtin(&name, &args, self.ctx)
                } else {
                    Ok(*self.ctx.vars.get(&name).unwrap_or(&0.0))
                }
            }
            _ => Err("Unexpected token in expression".into()),
        }
    }
}

fn call_builtin(name: &str, args: &[f64], _ctx: &EvalContext) -> Result<f64, String> {
    let a0 = args.first().copied().unwrap_or(0.0);
    let a1 = args.get(1).copied().unwrap_or(0.0);
    let a2 = args.get(2).copied().unwrap_or(0.0);
    let v = match name {
        "sin" => a0.sin(),
        "cos" => a0.cos(),
        "tan" => a0.tan(),
        "asin" => a0.clamp(-1.0, 1.0).asin(),
        "acos" => a0.clamp(-1.0, 1.0).acos(),
        "atan" => a0.atan(),
        "atan2" => a0.atan2(a1),
        "sinh" => a0.sinh(),
        "cosh" => a0.cosh(),
        "tanh" => a0.tanh(),
        "sqrt" => a0.max(0.0).sqrt(),
        "exp" => a0.clamp(-50.0, 50.0).exp(),
        "log" | "ln" => a0.max(1e-12).ln(),
        "log2" => a0.max(1e-12).log2(),
        "log10" => a0.max(1e-12).log10(),
        "abs" | "fabs" => a0.abs(),
        "floor" => a0.floor(),
        "ceil" => a0.ceil(),
        "round" => a0.round(),
        "min" => args.iter().copied().fold(f64::INFINITY, f64::min),
        "max" => args.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        "clamp" => a0.clamp(a1.min(a2), a1.max(a2)),
        "lerp" => a0 + (a1 - a0) * a2,
        "isn" | "eqr_isn" => eqr_isn(a0),
        "ics" | "eqr_ics" => eqr_ics(a0),
        "isn_inv" | "eqr_isn_inv" => eqr_isn_inv(a0),
        "book_isn" => book_isn(a0),
        "book_ics" => book_ics(a0),
        "book_isn_inv" => book_isn_inv(a0),
        "book_ics_inv" => book_ics_inv(a0),
        "ot_add" => ot_add(a0, a1),
        "ot_sub" => ot_sub(a0, a1),
        "ot_prod" => ot_prod(a0, a1),
        "ot_pow" => ot_pow(a0, a1),
        "ot_div" => ot_div(a0, a1),
        "P" => eqr_tensor_audio(a0, a0, a1, a2).p,
        "E" => eqr_tensor_audio(a0, a1, a0, a2).e,
        "D" => eqr_tensor_audio(a0, a1, a0, a2).d,
        "Z" => eqr_tensor_audio(a0, a1, a0, a2).z,
        _ => return Err(format!("Unknown function '{}'", name)),
    };
    Ok(if v.is_finite() { v } else { 0.0 })
}

pub fn eval_expr(expr: &str, ctx: &EvalContext) -> Result<f64, String> {
    let toks = tokenize(expr)?;
    if toks.is_empty() {
        return Ok(0.0);
    }
    let mut p = ExprParser { toks: &toks, pos: 0, ctx };
    let val = p.parse_ternary()?;
    Ok(if val.is_finite() { val } else { 0.0 })
}

/// Evaluate a Global Play script or multi-line seed script (supports `def ...:`,
/// local variable assignments `v = ...`, `if cond:` / `else:`, and `return ...`).
pub fn eval_script(script: &str, base_ctx: &EvalContext) -> Result<f64, String> {
    let mut ctx = base_ctx.clone();
    let mut last_val = 0.0;
    let mut skip_indent: Option<usize> = None;
    let mut last_if_taken = false;

    for raw_line in script.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("def ") {
            continue;
        }
        let indent = raw_line.len() - raw_line.trim_start().len();
        if let Some(si) = skip_indent {
            if indent > si {
                continue;
            } else {
                skip_indent = None;
            }
        }
        if let Some(rest) = trimmed.strip_prefix("if ") {
            let cond_str = rest.trim_end_matches(':').trim();
            let c = eval_expr(cond_str, &ctx)?;
            last_if_taken = c != 0.0;
            if !last_if_taken {
                skip_indent = Some(indent);
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("elif ") {
            if last_if_taken {
                skip_indent = Some(indent);
            } else {
                let cond_str = rest.trim_end_matches(':').trim();
                let c = eval_expr(cond_str, &ctx)?;
                last_if_taken = c != 0.0;
                if !last_if_taken {
                    skip_indent = Some(indent);
                }
            }
            continue;
        }
        if trimmed == "else:" {
            if last_if_taken {
                skip_indent = Some(indent);
            }
            continue;
        }
        if let Some(ret) = trimmed.strip_prefix("return ") {
            return eval_expr(ret.trim(), &ctx);
        }
        if let Some((lhs, rhs)) = split_assignment(trimmed) {
            let val = eval_expr(rhs, &ctx)?;
            ctx.vars.insert(lhs.to_string(), val);
            last_val = val;
            continue;
        }
        last_val = eval_expr(trimmed, &ctx)?;
    }
    Ok(last_val)
}

fn split_assignment(line: &str) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] == b'=' {
            let prev = if i > 0 { bytes[i - 1] } else { 0 };
            let next = bytes.get(i + 1).copied().unwrap_or(0);
            if !matches!(prev, b'=' | b'!' | b'<' | b'>') && next != b'=' {
                let lhs = line[..i].trim();
                let rhs = line[i + 1..].trim();
                if !lhs.is_empty() && lhs.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    return Some((lhs, rhs));
                }
            }
        }
    }
    None
}

/// Split a comma/newline-separated seed input while respecting parentheses.
pub fn split_seed_list_parts(raw: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut buf = String::new();
    let mut depth = 0i32;
    for ch in raw.replace("\r\n", "\n").chars() {
        match ch {
            '(' | '[' => {
                depth += 1;
                buf.push(ch);
            }
            ')' | ']' => {
                depth = (depth - 1).max(0);
                buf.push(ch);
            }
            ',' | '\n' if depth == 0 => {
                let t = buf.trim();
                if !t.is_empty() {
                    parts.push(t.to_string());
                }
                buf.clear();
            }
            _ => buf.push(ch),
        }
    }
    let t = buf.trim();
    if !t.is_empty() {
        parts.push(t.to_string());
    }
    parts
}

/// Parse a seed string (single float, comma-separated list, or math script) into finite floats.
pub fn parse_seed_numeric_list(raw: &str, t: f64, ot: bool) -> Vec<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let ctx = EvalContext::new(t, 0.0, 0, 120.0, ot);
    if trimmed.contains("def ") || trimmed.contains("return ") {
        if let Ok(v) = eval_script(trimmed, &ctx) {
            return vec![v];
        }
    }
    let mut out = Vec::new();
    for part in split_seed_list_parts(trimmed) {
        if let Ok(n) = part.parse::<f64>() {
            if n.is_finite() {
                out.push(n);
            }
        } else if let Ok(v) = eval_expr(&part, &ctx) {
            if v.is_finite() {
                out.push(v);
            }
        }
    }
    out
}

/// Return semantic hints for a Domain Algo equation string (mirrors README / Global Play Panel).
pub fn domain_equation_hints(eq: &str) -> Vec<&'static str> {
    let low = eq.to_lowercase();
    let mut hints = Vec::new();
    if low.contains("sin") || low.contains("cos") || low.contains("isn") || low.contains("ics") {
        hints.push("phase-oscillator");
    }
    if low.contains("log") || low.contains("exp") || low.contains("sqrt") {
        hints.push("scale-warp");
    }
    if low.contains("meum") || low.contains("phi") {
        hints.push("meum-phi-lattice");
    }
    if low.contains("ot_") {
        hints.push("operator-theory-symbolic");
    }
    if hints.is_empty() {
        hints.push("transmutor-linear");
    }
    hints
}
