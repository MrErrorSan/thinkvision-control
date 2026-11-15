#!/usr/bin/env python3
"""Extract the @tr() strings from .slint files into a gettext template (.pot).

This mirrors `slint-tr-extractor` 1.18.1 (the Slint project's extractor, main.rs), for machines where it can't be
built. Use the real tool when you can:

    cargo install slint-tr-extractor --version 1.18.1 --locked
    slint-tr-extractor --package-name "ThinkVision Control" --package-version 0.1.0 \
        -o translations/tvc-app.pot crates/tvc-app/ui/*.slint

Otherwise, from the repository root:

    python -I scripts/extract-slint-strings.py -o translations/tvc-app.pot crates/tvc-app/ui/*.slint

What it mirrors:
- msgctxt is the explicit context (`@tr("ctx" => "msg")`), else the name of the enclosing `component` or `global`.
  That is the context slint-build gives each string by default (DefaultTranslationContext::ComponentName).
- msgid_plural comes from `@tr("one" | "many" % n)`, with two empty msgstr[] slots.
- `#.` is the `//` comment that ends the line directly above the @tr line; `#:` lists every path:line.
- The same (msgctxt, msgid, msgid_plural) is one entry, in order of first appearance.
- The header carries the same fields, including "Plural-Forms: nplurals=1; plural=0;".
"""

import argparse
import datetime
import sys

WRAP = 78


class Entry:
    def __init__(self, ctx, msgid, plural):
        self.ctx, self.msgid, self.plural = ctx, msgid, plural
        self.comment = None
        self.occurrences = []


def unescape(s, path, line):
    out, i = [], 0
    while i < len(s):
        c = s[i]
        if c != "\\":
            out.append(c)
            i += 1
            continue
        n = s[i + 1]
        if n == "u" and s[i + 2] == "{":
            end = s.index("}", i)
            out.append(chr(int(s[i + 3:end], 16)))
            i = end + 1
            continue
        if n == "{":
            sys.exit(f"{path}:{line}: string interpolation inside @tr() isn't supported; use {{}} placeholders")
        out.append({"n": "\n", "t": "\t", "\\": "\\", '"': '"'}.get(n, n))
        i += 2
    return "".join(out)


def tokenize(text):
    """Yield (kind, value, line, offset): kinds are str, comment, ident, punct. Whitespace is dropped."""
    i, line, n = 0, 1, len(text)
    while i < n:
        c = text[i]
        if c == "\n":
            line += 1
            i += 1
        elif c in " \t\r":
            i += 1
        elif text.startswith("//", i):
            end = text.find("\n", i)
            end = n if end < 0 else end
            yield "comment", text[i:end], line, i
            i = end
        elif text.startswith("/*", i):
            end = text.index("*/", i) + 2
            yield "comment", text[i:end], line, i
            line += text.count("\n", i, end)
            i = end
        elif c == '"':
            j = i + 1
            while text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            yield "str", text[i + 1:j], line, i
            line += text.count("\n", i, j)
            i = j + 1
        elif c.isalnum() or c == "_":
            j = i
            while j < n and (text[j].isalnum() or text[j] in "_-"):
                j += 1
            yield "ident", text[i:j], line, i
            i = j
        elif text.startswith("=>", i):
            yield "punct", "=>", line, i
            i += 2
        else:
            yield "punct", c, line, i
            i += 1


def comment_above(lines, line):
    """The `//` comment that ends the line above `line` (1-based), as slint-tr-extractor finds it."""
    if line < 2:
        return None
    prev = lines[line - 2]
    if not prev.strip():
        return None
    # The last `//` outside a string on that line.
    in_str, k, start = False, 0, None
    while k < len(prev):
        ch = prev[k]
        if in_str:
            if ch == "\\":
                k += 1
            elif ch == '"':
                in_str = False
        elif ch == '"':
            in_str = True
        elif prev.startswith("//", k):
            start = k
            break
        k += 1
    if start is None:
        return None
    return prev[start:].lstrip("/").strip()


def process(path, entries, index):
    text = open(path, encoding="utf-8").read()
    lines = text.split("\n")
    toks = list(tokenize(text))
    depth, component, pending = 0, None, None
    i = 0
    while i < len(toks):
        kind, val, line, _ = toks[i]
        if kind == "ident" and val in ("component", "global") and depth == 0 and i + 1 < len(toks):
            pending = toks[i + 1][1]
        elif kind == "punct" and val == "{":
            if depth == 0 and pending:
                component, pending = pending, None
            depth += 1
        elif kind == "punct" and val == "}":
            depth -= 1
            if depth == 0:
                component = None
        elif kind == "punct" and val == "@" and i + 2 < len(toks) and toks[i + 1][1] == "tr" and toks[i + 2][1] == "(":
            j = i + 3
            while toks[j][0] == "comment":
                j += 1
            ctx, plural = component, None
            if toks[j][0] != "str":
                sys.exit(f"{path}:{line}: @tr() must start with a string literal")
            if toks[j + 1][1] == "=>":
                ctx = unescape(toks[j][1], path, line)
                j += 2
            msg_tok = toks[j]
            msgid = unescape(msg_tok[1], path, msg_tok[2])
            if toks[j + 1][1] == "|":
                plural = unescape(toks[j + 2][1], path, line)
            key = (ctx, msgid, plural)
            e = index.get(key)
            if e is None:
                e = index[key] = Entry(ctx, msgid, plural)
                entries.append(e)
            occ = (path, toks[i][2])
            if occ not in e.occurrences:
                e.occurrences.append(occ)
            if not e.comment:
                e.comment = comment_above(lines, msg_tok[2]) or comment_above(lines, toks[i][2])
            # Nested @tr() inside the arguments is picked up as the scan continues.
            i += 3
            continue
        i += 1


def po_escape(s):
    return s.replace("\\", "\\\\").replace('"', '\\"').replace("\t", "\\t").replace("\r", "\\r").replace("\n", "\\n")


def po_field(name, value):
    """`name "value"`, wrapped like gettext/polib when it's longer than 78 columns."""
    esc = po_escape(value)
    one = f'{name} "{esc}"'
    if len(one) <= WRAP and "\\n" not in esc[:-2]:
        return [one]
    out = [f'{name} ""']
    # Split after each \n, then on spaces so each line fits.
    pieces = []
    for part in esc.replace("\\n", "\\n\0").split("\0"):
        if part:
            pieces.append(part)
    for piece in pieces:
        cur = ""
        for word in piece.split(" "):
            cand = word if not cur else cur + " " + word
            if len(cand) + 2 > WRAP - 1 and cur:
                out.append(f'"{cur} "')
                cur = word
            else:
                cur = cand
        if cur:
            out.append(f'"{cur}"')
    return out


def render(entries, package, version):
    now = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%d %H:%M%z")
    header = [
        ("Project-Id-Version", f"{package} {version}"),
        ("POT-Creation-Date", now),
        ("PO-Revision-Date", "YEAR-MO-DA HO:MI+ZONE"),
        ("Last-Translator", "FULL NAME <EMAIL@ADDRESS>"),
        ("Language-Team", "LANGUAGE <LL@li.org>"),
        ("MIME-Version", "1.0"),
        ("Content-Type", "text/plain; charset=UTF-8"),
        ("Content-Transfer-Encoding", "8bit"),
        ("Language", ""),
        ("Plural-Forms", "nplurals=1; plural=0;"),
    ]
    out = ['msgid ""', 'msgstr ""']
    out += [f'"{po_escape(k + ": " + v)}\\n"' for k, v in header]
    for e in entries:
        out.append("")
        if e.comment:
            out.append(f"#. {e.comment}")
        refs = " ".join(f"{p}:{l}" for p, l in e.occurrences)
        out.append(f"#: {refs}")
        if e.ctx is not None:
            out += po_field("msgctxt", e.ctx)
        out += po_field("msgid", e.msgid)
        if e.plural is not None:
            out += po_field("msgid_plural", e.plural)
            out += ['msgstr[0] ""', 'msgstr[1] ""']
        else:
            out.append('msgstr ""')
    return "\n".join(out) + "\n"


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("paths", nargs="+")
    ap.add_argument("-o", dest="output", default="messages.po")
    ap.add_argument("--package-name", default="PACKAGE")
    ap.add_argument("--package-version", default="VERSION")
    args = ap.parse_args()
    entries, index = [], {}
    for p in args.paths:
        process(p.replace("\\", "/"), entries, index)
    with open(args.output, "w", encoding="utf-8", newline="\n") as f:
        f.write(render(entries, args.package_name, args.package_version))
    print(f"{args.output}: {len(entries)} entries")


if __name__ == "__main__":
    main()
