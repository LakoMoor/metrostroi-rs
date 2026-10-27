//! Lua host: runs Metrostroi's original Lua (Lua 5.1, like GLua) against a small GMod shim.

mod glua;

use anyhow::{Context, Result};
use mlua::{Lua, Value};
use std::path::{Path, PathBuf};

const GMOD_SHIM: &str = include_str!("gmod.lua");
const ENTITY_HOST: &str = include_str!("entity.lua");

pub struct Runtime {
    pub lua: Lua,
    root: PathBuf,
}

/// GLua-only operators that Lua 5.1 rejects (`!=`, `&&`, `||`), rewritten outside strings and comments.
fn preprocess(src: &str) -> String {
    let src = src.trim_start_matches('\u{feff}');
    let rewritten = if src.contains("continue") { glua::rewrite_continue(src) } else { String::new() };
    let src = if rewritten.is_empty() { src } else { rewritten.as_str() };
    let b = src.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'"' || c == b'\'' {
            let q = c;
            out.push(c);
            i += 1;
            while i < b.len() && b[i] != q {
                if b[i] == b'\\' && i + 1 < b.len() {
                    out.push(b[i]);
                    i += 1;
                }
                if b[i] == b'\n' {
                    break;
                }
                out.push(b[i]);
                i += 1;
            }
            if i < b.len() {
                out.push(b[i]);
                i += 1;
            }
        } else if c == b'-' && b.get(i + 1) == Some(&b'-') {
            // comment: long (--[[ ]] / --[=[ ]=]) or to end of line
            let mut level = None;
            if b.get(i + 2) == Some(&b'[') {
                let mut j = i + 3;
                while b.get(j) == Some(&b'=') {
                    j += 1;
                }
                if b.get(j) == Some(&b'[') {
                    level = Some(j - i - 3);
                }
            }
            let end = match level {
                Some(n) => {
                    let close = format!("]{}]", "=".repeat(n));
                    src[i..].find(&close).map_or(b.len(), |p| i + p + close.len())
                }
                None => src[i..].find('\n').map_or(b.len(), |p| i + p),
            };
            let text = &src[i..end];
            if level == Some(0) && text.len() >= 8 && text.ends_with("]]") && text[4..text.len() - 2].contains("[[") && !text.contains("]=]") {
                out.extend_from_slice(b"--[=[");
                out.extend_from_slice(text[4..text.len() - 2].as_bytes());
                out.extend_from_slice(b"]=]");
            } else {
                out.extend_from_slice(text.as_bytes());
            }
            i = end;
        } else if c == b'[' && b.get(i + 1) == Some(&b'[') {
            // long string; Lua 5.1 rejects a nested `[[` inside one, so re-bracket those as `[=[ ... ]=]`
            let end = src[i + 2..].find("]]").map_or(b.len(), |p| i + 2 + p + 2);
            let body = &src[i + 2..end.saturating_sub(2).max(i + 2)];
            if body.contains("[[") && !body.contains("]=]") {
                out.extend_from_slice(b"[=[");
                out.extend_from_slice(body.as_bytes());
                out.extend_from_slice(b"]=]");
            } else {
                out.extend_from_slice(&b[i..end]);
            }
            i = end;
        } else if c == b'!' && b.get(i + 1) == Some(&b'=') {
            out.extend_from_slice(b"~=");
            i += 2;
        } else if c == b'!' {
            out.extend_from_slice(b" not ");
            i += 1;
        } else if c == b'&' && b.get(i + 1) == Some(&b'&') {
            out.extend_from_slice(b" and ");
            i += 2;
        } else if c == b'|' && b.get(i + 1) == Some(&b'|') {
            out.extend_from_slice(b" or ");
            i += 2;
        } else {
            out.push(c);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn install_bit(lua: &Lua) -> mlua::Result<()> {
    let bit = lua.create_table()?;
    let i = |v: f64| v as i64 as i32;
    bit.set("tobit", lua.create_function(move |_, a: f64| Ok(i(a) as f64))?)?;
    bit.set("bnot", lua.create_function(move |_, a: f64| Ok((!i(a)) as f64))?)?;
    bit.set("band", lua.create_function(move |_, a: mlua::Variadic<f64>| Ok(a.iter().fold(-1i32, |x, v| x & i(*v)) as f64))?)?;
    bit.set("bor", lua.create_function(move |_, a: mlua::Variadic<f64>| Ok(a.iter().fold(0i32, |x, v| x | i(*v)) as f64))?)?;
    bit.set("bxor", lua.create_function(move |_, a: mlua::Variadic<f64>| Ok(a.iter().fold(0i32, |x, v| x ^ i(*v)) as f64))?)?;
    bit.set("lshift", lua.create_function(move |_, (a, n): (f64, f64)| Ok(i(a).wrapping_shl(n as u32 & 31) as f64))?)?;
    bit.set("rshift", lua.create_function(move |_, (a, n): (f64, f64)| Ok(((i(a) as u32) >> (n as u32 & 31)) as i32 as f64))?)?;
    bit.set("arshift", lua.create_function(move |_, (a, n): (f64, f64)| Ok((i(a) >> (n as u32 & 31)) as f64))?)?;
    bit.set("rol", lua.create_function(move |_, (a, n): (f64, f64)| Ok((i(a) as u32).rotate_left(n as u32 & 31) as i32 as f64))?)?;
    bit.set("ror", lua.create_function(move |_, (a, n): (f64, f64)| Ok((i(a) as u32).rotate_right(n as u32 & 31) as i32 as f64))?)?;
    lua.globals().set("bit", bit)
}

/// Files in `dir` matching `prefix*suffix` (the only glob shape Metrostroi uses), sorted.
fn find_files(root: &Path, pattern: &str) -> Vec<String> {
    let (dir, file) = pattern.rsplit_once('/').unwrap_or(("", pattern));
    let (prefix, suffix) = file.split_once('*').unwrap_or((file, ""));
    let mut v: Vec<String> = std::fs::read_dir(root.join(dir))
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with(prefix) && n.ends_with(suffix))
        .collect();
    v.sort();
    v
}

impl Runtime {
    /// `root` is the folder that plays the role of GMod's `lua/` directory.
    pub fn new(root: &Path) -> Result<Self> {
        let lua = Lua::new();
        install_bit(&lua)?;
        let g = lua.globals();
        let r = root.to_path_buf();
        g.set(
            "__read_lua",
            lua.create_function(move |lua, p: String| {
                let p = p.replace('\\', "/");
                Ok(match std::fs::read(r.join(p)) {
                    Ok(b) => Value::String(lua.create_string(&b)?),
                    Err(_) => Value::Nil,
                })
            })?,
        )?;
        let r = root.to_path_buf();
        g.set("__find_lua", lua.create_function(move |_, p: String| Ok(find_files(&r, &p)))?)?;
        g.set("__gmod_preprocess", lua.create_function(|_, s: mlua::String| Ok(preprocess(&s.to_string_lossy())))?)?;
        lua.load(GMOD_SHIM).set_name("@gmod.lua").exec().context("gmod shim")?;
        lua.load(ENTITY_HOST).set_name("@entity.lua").exec().context("entity host")?;
        Ok(Runtime { lua, root: root.to_path_buf() })
    }

    /// Shared Metrostroi helper files the entity code relies on.
    pub fn include(&self, path: &str) -> Result<()> {
        let inc: mlua::Function = self.lua.globals().get("include")?;
        inc.call::<Value>(path.to_string()).with_context(|| path.to_string())?;
        Ok(())
    }

    pub fn spawn_train(&self, class: &str) -> Result<mlua::Table> {
        let m: mlua::Table = self.lua.globals().get("Metrostroi")?;
        let f: mlua::Function = m.get("SpawnTrain")?;
        Ok(f.call::<mlua::Table>(class.to_string())?)
    }

    pub fn exec(&self, code: &str, name: &str) -> Result<()> {
        self.lua.load(&preprocess(code)).set_name(name).exec()?;
        Ok(())
    }

    /// Load every `metrostroi/systems/sys_*.lua`; returns (loaded, failures).
    pub fn load_systems(&self) -> (usize, Vec<(String, String)>) {
        let load: mlua::Function = self.lua.globals().get::<mlua::Table>("Metrostroi").unwrap().get("LoadSystemFile").unwrap();
        let (mut ok, mut bad) = (0, vec![]);
        for f in find_files(&self.root, "metrostroi/systems/sys_*.lua") {
            let path = format!("metrostroi/systems/{f}");
            match load.call::<Value>(path) {
                Ok(_) => ok += 1,
                Err(e) => bad.push((f, e.to_string().lines().next().unwrap_or("").to_string())),
            }
        }
        (ok, bad)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preprocess_rewrites_outside_strings() {
        assert_eq!(preprocess("if a != b && c || d then x = '!=' end -- a != b"), "if a ~= b  and  c  or  d then x = '!=' end -- a != b");
    }
}
