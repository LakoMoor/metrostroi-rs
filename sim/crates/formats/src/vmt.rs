//! Minimal Valve material (`.vmt`) reader: shader name and flat `$parameters`.

use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct Vmt {
    pub shader: String,
    /// Lower-case keys, values as written (quotes removed).
    pub params: HashMap<String, String>,
}

impl Vmt {
    pub fn base_texture(&self) -> Option<&str> {
        self.params.get("$basetexture").map(|s| s.as_str())
    }
    pub fn flag(&self, key: &str) -> bool {
        self.params.get(key).map_or(false, |v| v.trim() != "0")
    }
}

fn tokens(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut it = src.chars().peekable();
    while let Some(&c) = it.peek() {
        if c.is_whitespace() {
            it.next();
        } else if c == '/' {
            // "//" comment to end of line
            it.next();
            if it.peek() == Some(&'/') {
                while let Some(&d) = it.peek() {
                    if d == '\n' {
                        break;
                    }
                    it.next();
                }
            }
        } else if c == '"' {
            it.next();
            let mut s = String::new();
            for d in it.by_ref() {
                if d == '"' {
                    break;
                }
                s.push(d);
            }
            out.push(s);
        } else if c == '{' || c == '}' {
            out.push(c.to_string());
            it.next();
        } else {
            let mut s = String::new();
            while let Some(&d) = it.peek() {
                if d.is_whitespace() || d == '{' || d == '}' || d == '"' {
                    break;
                }
                s.push(d);
                it.next();
            }
            out.push(s);
        }
    }
    out
}

pub fn parse(src: &str) -> Vmt {
    let t = tokens(src);
    let mut v = Vmt::default();
    let mut i = 0;
    if let Some(s) = t.first() {
        v.shader = s.to_ascii_lowercase();
        i = 1;
    }
    let mut depth = 0;
    while i < t.len() {
        match t[i].as_str() {
            "{" => depth += 1,
            "}" => depth -= 1,
            key if depth == 1 && i + 1 < t.len() && t[i + 1] != "{" && t[i + 1] != "}" => {
                v.params.insert(key.to_ascii_lowercase(), t[i + 1].clone());
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    v
}
