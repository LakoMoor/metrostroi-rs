"""Extract ClientProps and ButtonMap tables from a Metrostroi entity cl_init.lua into JSON.

Usage: extract_controls.py <cl_init.lua> <out.json>
Only literal tables are read (Vector/Angle/number/string fields); entries built by loops or
table.Copy are recorded as 'derived' so they are not silently lost.
"""
import ast
import json
import operator
import re
import sys
from pathlib import Path

HEAD = re.compile(r'ENT\.(ClientProps|ButtonMap)\["([^"]+)"\]\s*=\s*(table\.Copy\(.*?\)|\{)')
VEC = re.compile(r'(Vector|Angle)\(([^)]*)\)')


def block(src, start):
    """src[start] is '{'; return the text up to the matching '}'."""
    depth = 0
    for i in range(start, len(src)):
        c = src[i]
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                return src[start:i + 1]
    raise ValueError("unbalanced braces")


_OPS = {ast.Add: operator.add, ast.Sub: operator.sub, ast.Mult: operator.mul, ast.Div: operator.truediv}


def eval_num(expr):
    """Evaluate a literal arithmetic expression (numbers, + - * / and parentheses); else None."""
    def ev(n):
        if isinstance(n, ast.Expression):
            return ev(n.body)
        if isinstance(n, ast.Constant) and isinstance(n.value, (int, float)):
            return float(n.value)
        if isinstance(n, ast.UnaryOp) and isinstance(n.op, (ast.USub, ast.UAdd)):
            v = ev(n.operand)
            return -v if isinstance(n.op, ast.USub) else v
        if isinstance(n, ast.BinOp) and type(n.op) in _OPS:
            return _OPS[type(n.op)](ev(n.left), ev(n.right))
        raise ValueError
    try:
        return ev(ast.parse(expr.strip(), mode="eval"))
    except (ValueError, SyntaxError, ZeroDivisionError):
        return None


def split_args(s):
    """Split on top-level commas."""
    out, depth, cur = [], 0, ""
    for ch in s:
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
        if ch == "," and depth == 0:
            out.append(cur)
            cur = ""
        else:
            cur += ch
    out.append(cur)
    return out


def nums(s):
    out = []
    for p in split_args(s):
        v = eval_num(p)
        out.append(v if v is not None else p.strip())
    return out


LOCAL = re.compile(r'^local\s+(\w+)\s*=\s*(Vector|Angle)\(([^)]*)\)', re.M)


def fields(body, offset=0, src="", props=None):
    """Top-level key = value pairs of a table body (nested tables skipped)."""
    res, depth, i = {}, 0, 1
    flat = []
    for ch in body[1:-1]:
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
        flat.append(ch if depth == 0 else " ")
    text = "".join(flat)
    for m in re.finditer(r'(\w+)\s*=\s*(Vector\((?:[^()]|\([^()]*\))*\)|Angle\((?:[^()]|\([^()]*\))*\)|ENT\.ClientProps(?:\["[^"]+"\]|\.\w+)\.\w+|"[^"]*"|-?[\w.]+)', text):
        k, v = m.group(1), m.group(2)
        ref = re.fullmatch(r'ENT\.ClientProps(?:\["([^"]+)"\]|\.(\w+))\.(\w+)', v)
        if ref and props is not None:
            target, field = ref.group(1) or ref.group(2), ref.group(3)
            if target in props and field in props[target]:
                res[k] = props[target][field]
                continue
        if k in ("pos", "ang") and re.fullmatch(r"[A-Za-z_]\w*", v) and src:
            at = offset + m.start()
            cands = [l for l in LOCAL.finditer(src) if l.group(1) == v and l.start() < at]
            if cands:
                l = cands[-1]
                res[k] = {l.group(2).lower(): nums(l.group(3))}
                continue
        vm = re.fullmatch(r'(Vector|Angle)\(((?:[^()]|\([^()]*\))*)\)', v)
        if vm:
            res[k] = {vm.group(1).lower(): nums(vm.group(2))}
        elif v.startswith('"'):
            res[k] = v[1:-1]
        else:
            try:
                res[k] = float(v)
            except ValueError:
                res[k] = v
    return res


def buttons(body):
    m = re.search(r'buttons\s*=\s*\{', body)
    if not m:
        return []
    bb = block(body, m.end() - 1)
    out = []
    for em in re.finditer(r'\{', bb[1:]):
        pass
    i = 1
    while i < len(bb) - 1:
        if bb[i] == "{":
            eb = block(bb, i)
            f = fields(eb)
            if "ID" in f:
                out.append(f)
            i += len(eb)
        else:
            i += 1
    return out


def main(src_path, out_path):
    src = Path(src_path).read_text("latin-1")
    props, panels, derived = {}, {}, []
    for m in HEAD.finditer(src):
        kind, name, rhs = m.groups()
        if rhs.startswith("table.Copy"):
            derived.append({"kind": kind, "name": name, "expr": rhs})
            continue
        body = block(src, m.end() - 1)
        if kind == "ClientProps":
            props[name] = fields(body, m.end() - 1, src, props)
        else:
            f = fields(body)
            f["buttons"] = buttons(body)
            panels[name] = f
    out = {"client_props": props, "button_panels": panels, "derived": derived}
    Path(out_path).parent.mkdir(parents=True, exist_ok=True)
    Path(out_path).write_text(json.dumps(out, indent=1))
    nb = sum(len(p["buttons"]) for p in panels.values())
    print(f"props={len(props)} panels={len(panels)} buttons={nb} derived={len(derived)}")


if __name__ == "__main__":
    main(*sys.argv[1:3])
