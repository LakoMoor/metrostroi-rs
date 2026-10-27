//! GLua's `continue`: wrap the enclosing loop body in `repeat ... until true` and turn `continue` into `break`.

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Loop,
    Func,
    Block,
    Repeat,
}

/// End of a string literal, comment or long string starting at `i`, or `None` if `b[i]` starts none.
fn skip_literal(src: &str, i: usize) -> Option<usize> {
    let b = src.as_bytes();
    let long_close = |open: usize| -> Option<(usize, String)> {
        // `open` points at the first '['; returns the closing sequence if this is a long bracket
        let mut j = open + 1;
        while b.get(j) == Some(&b'=') {
            j += 1;
        }
        (b.get(j) == Some(&b'[')).then(|| (j, format!("]{}]", "=".repeat(j - open - 1))))
    };
    match b[i] {
        q @ (b'"' | b'\'') => {
            let mut j = i + 1;
            while j < b.len() && b[j] != q && b[j] != b'\n' {
                if b[j] == b'\\' {
                    j += 1;
                }
                j += 1;
            }
            Some(j + 1)
        }
        b'-' if b.get(i + 1) == Some(&b'-') => {
            if b.get(i + 2) == Some(&b'[') {
                if let Some((j, close)) = long_close(i + 2) {
                    return Some(src[j..].find(&close).map_or(b.len(), |p| j + p + close.len()));
                }
            }
            Some(src[i..].find('\n').map_or(b.len(), |p| i + p))
        }
        b'[' => {
            let (j, close) = long_close(i)?;
            Some(src[j..].find(&close).map_or(b.len(), |p| j + p + close.len()))
        }
        _ => None,
    }
}

pub fn rewrite_continue(src: &str) -> String {
    let b = src.as_bytes();
    // (kind, position after `do`, loop body contains `continue`)
    let mut stack: Vec<(Kind, usize, bool)> = vec![];
    let mut edits: Vec<(usize, usize, &'static str)> = vec![];
    let mut pending_loop = false;
    let mut i = 0;
    while i < b.len() {
        if let Some(end) = skip_literal(src, i) {
            i = end;
            continue;
        }
        let c = b[i];
        if !(c.is_ascii_alphabetic() || c == b'_') {
            i += 1;
            continue;
        }
        let s = i;
        while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
            i += 1;
        }
        if s > 0 && (b[s - 1] == b'.' || b[s - 1] == b':') {
            continue;
        }
        match &src[s..i] {
            "for" | "while" => pending_loop = true,
            "do" => {
                let kind = if pending_loop { Kind::Loop } else { Kind::Block };
                pending_loop = false;
                stack.push((kind, i, false));
            }
            "if" => stack.push((Kind::Block, i, false)),
            "function" => stack.push((Kind::Func, i, false)),
            "repeat" => stack.push((Kind::Repeat, i, false)),
            "until" => {
                stack.pop();
            }
            "end" => {
                if let Some((Kind::Loop, after_do, true)) = stack.pop() {
                    edits.push((after_do, after_do, " repeat "));
                    edits.push((s, s, " until true "));
                }
            }
            "continue" => {
                edits.push((s, i, "break"));
                if let Some(l) = stack.iter_mut().rev().take_while(|e| e.0 != Kind::Func).find(|e| e.0 == Kind::Loop) {
                    l.2 = true;
                }
            }
            _ => {}
        }
    }
    edits.sort_by_key(|e| e.0);
    let mut out = String::with_capacity(src.len() + 64);
    let mut last = 0;
    for (s, e, r) in edits {
        out.push_str(&src[last..s]);
        out.push_str(r);
        last = e;
    }
    out.push_str(&src[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continue_wraps_loop() {
        let out = rewrite_continue("for i=1,3 do if i==2 then continue end print(i) end");
        assert_eq!(out, "for i=1,3 do repeat  if i==2 then break end print(i)  until true end");
    }

    #[test]
    fn continue_in_comment_untouched() {
        let s = "for i=1,3 do --[[ continue ]] print('continue') end -- continue";
        assert_eq!(rewrite_continue(s), s);
    }
}
