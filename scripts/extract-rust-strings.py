#!/usr/bin/env python3
"""Extract the text built in Rust into a gettext template (translations/tvc.pot), for tvc_core::i18n.

From the repository root:

    python -I scripts/extract-rust-strings.py

It writes translations/tvc.pot (or the file given with -o). Standard library only.

What it collects:
- The literal first argument of `tr("…")` and `trf("…", …)`, and the two literal arguments of `trn("…", "…", …)`
  (msgid and msgid_plural), in crates/tvc-app/src and crates/tvc-core/src. Rust string escapes and
  backslash-newline continuations are decoded as rustc does; raw strings (r"…", r#"…"#) are read as they are.
  Comments are skipped, so a `tr("…")` in a doc comment is not an entry. A call whose argument isn't a literal
  (`tr(&def.name)`) is skipped: its text comes from one of the sources below.
- The profile texts the app passes through `tr` at run time: each control's `name`, `description`, `confirm`,
  `confirmTitle`, `confirmBody`, `confirmButton` and value labels, each input's `label`, and the profile's
  `brightnessLockNote`, from crates/tvc-core/src/profile/*.json.
- The default preset names "Day" and "Night" (crates/tvc-core/src/store.rs) and the shortcut action labels, the
  second field of the `("Key", "Label", "Gesture")` tuples in crates/tvc-core/src/hotkey.rs.

The same msgid is one entry, in order of first appearance (files in sorted order), with every `#: path:line`.
Placeholders are tvc_core::i18n's positional `{0}`, `{1}`: a translation keeps each one and may reorder them.
"""

import argparse
import datetime
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SOURCES = ["crates/tvc-app/src", "crates/tvc-core/src"]
PROFILES = "crates/tvc-core/src/profile"
STORE = "crates/tvc-core/src/store.rs"
HOTKEY = "crates/tvc-core/src/hotkey.rs"
FUNCTIONS = {"tr": 1, "trf": 1, "trn": 2}
WRAP = 78


class Entry:
    def __init__(self, msgid, plural):
        self.msgid, self.plural = msgid, plural
        self.refs = []


class Catalog:
    def __init__(self):
        self.entries = {}

    def add(self, msgid, plural, ref):
        if msgid == "":
            return
        entry = self.entries.get(msgid)
        if entry is None:
            entry = self.entries[msgid] = Entry(msgid, plural)
        elif plural and not entry.plural:
            entry.plural = plural
        elif plural and entry.plural != plural:
            sys.exit(f"{ref}: {msgid!r} has two plurals: {entry.plural!r} and {plural!r}")
        if ref not in entry.refs:
            entry.refs.append(ref)


# ---------------------------------------------------------------------------------------------------------------
# Rust


def unescape(body, where):
    """A normal (non-raw) Rust string literal's body, decoded."""
    out, i, n = [], 0, len(body)
    while i < n:
        c = body[i]
        if c != "\\":
            out.append(c)
            i += 1
            continue
        nxt = body[i + 1] if i + 1 < n else ""
        if nxt == "\n" or (nxt == "\r" and body[i + 2:i + 3] == "\n"):
            # Line continuation: the newline and the whitespace after it are dropped.
            i += 2
            while i < n and body[i] in " \t\r\n":
                i += 1
            continue
        simple = {"n": "\n", "t": "\t", "r": "\r", "0": "\0", "\\": "\\", "'": "'", '"': '"'}
        if nxt in simple:
            out.append(simple[nxt])
            i += 2
        elif nxt == "x":
            out.append(chr(int(body[i + 2:i + 4], 16)))
            i += 4
        elif nxt == "u" and body[i + 2:i + 3] == "{":
            end = body.index("}", i)
            out.append(chr(int(body[i + 3:end].replace("_", ""), 16)))
            i = end + 1
        else:
            sys.exit(f"{where}: unknown escape \\{nxt}")
    return "".join(out)


def tokens(text, path):
    """(kind, value, line) for identifiers ("id"), string literals ("str"), and single punctuation ("p")."""
    i, n, line = 0, len(text), 1
    while i < n:
        c = text[i]
        if c == "\n":
            line += 1
            i += 1
        elif c in " \t\r":
            i += 1
        elif text.startswith("//", i):
            end = text.find("\n", i)
            i = n if end < 0 else end
        elif text.startswith("/*", i):
            depth, i = 1, i + 2
            while i < n and depth:
                if text.startswith("/*", i):
                    depth, i = depth + 1, i + 2
                elif text.startswith("*/", i):
                    depth, i = depth - 1, i + 2
                else:
                    line += text[i] == "\n"
                    i += 1
        elif c == '"' or (c == "b" and text.startswith('b"', i)):
            start_line = line
            i += 1 if c == '"' else 2
            start = i
            while text[i] != '"':
                if text[i] == "\\":
                    i += 1
                line += text[i] == "\n"
                i += 1
            body = text[start:i]
            i += 1
            yield ("str", unescape(body, f"{path}:{start_line}"), start_line)
        elif c in "rb" and re.match(r'(br|r)(#*)"', text[i:i + 300]):
            m = re.match(r'(br|r)(#*)"', text[i:])
            close = '"' + m.group(2)
            start = i + m.end()
            end = text.index(close, start)
            body = text[start:end]
            yield ("str", body, line)
            line += body.count("\n")
            i = end + len(close)
        elif c == "'":
            # A char literal ('a', '\n', '\u{10}') or a lifetime ('a, 'static).
            if text[i + 1] == "\\":
                i = text.index("'", i + 3) + 1  # past the escaped character, which may be a quote
            elif i + 2 < n and text[i + 2] == "'":
                i += 3
            else:
                i += 1
        elif c.isalpha() or c == "_":
            m = re.match(r"[A-Za-z_][A-Za-z0-9_]*", text[i:])
            yield ("id", m.group(0), line)
            i += m.end()
        elif c.isdigit():
            m = re.match(r"[0-9][0-9A-Za-z_.]*", text[i:])
            i += m.end()
        else:
            yield ("p", c, line)
            i += 1


def scan_rust(catalog, rel):
    text = open(os.path.join(ROOT, rel), encoding="utf-8").read()
    toks = list(tokens(text, rel))
    for k, (kind, value, line) in enumerate(toks):
        if kind != "id" or value not in FUNCTIONS:
            continue
        if k > 0 and toks[k - 1][:2] == ("id", "fn"):
            continue  # the definition
        if toks[k + 1][:2] != ("p", "("):
            continue
        first = toks[k + 2]
        if first[0] != "str":
            continue  # not a literal: comes from a profile, a setting or another source
        plural = None
        if FUNCTIONS[value] == 2:
            if toks[k + 3][:2] != ("p", ",") or toks[k + 4][0] != "str":
                sys.exit(f"{rel}:{line}: trn needs two literal arguments")
            plural = toks[k + 4][1]
        catalog.add(first[1], plural, f"{rel}:{line}")


def rust_files():
    for base in SOURCES:
        for dirpath, dirs, files in os.walk(os.path.join(ROOT, base)):
            dirs.sort()
            for name in sorted(files):
                if name.endswith(".rs"):
                    yield os.path.relpath(os.path.join(dirpath, name), ROOT).replace(os.sep, "/")


# ---------------------------------------------------------------------------------------------------------------
# Other sources


def scan_profiles(catalog):
    folder = os.path.join(ROOT, PROFILES)
    for name in sorted(os.listdir(folder)):
        if not name.endswith(".json"):
            continue
        rel = f"{PROFILES}/{name}"
        text = open(os.path.join(folder, name), encoding="utf-8").read()
        data = json.loads(text)
        cursor = 0

        def add(value):
            """Adds `value` at the line where it next appears (the values are visited in file order)."""
            nonlocal cursor
            if not isinstance(value, str) or not value:
                return
            needle = json.dumps(value, ensure_ascii=False)
            at = text.find(needle, cursor)
            if at < 0:
                at = max(text.find(needle), 0)
            cursor = at + len(needle)
            catalog.add(value, None, f"{rel}:{text.count(chr(10), 0, at) + 1}")

        add(data.get("brightnessLockNote"))
        for control in data.get("controls", []):
            for key in ("name", "description", "confirm", "confirmTitle", "confirmBody", "confirmButton"):
                add(control.get(key))
            for label in (control.get("values") or {}).values():
                add(label)
        for item in data.get("inputs", []):
            add(item.get("label"))


def scan_store(catalog):
    text = open(os.path.join(ROOT, STORE), encoding="utf-8").read()
    for m in re.finditer(r'Preset \{ name: "([^"]+)"\.into\(\)', text):
        catalog.add(m.group(1), None, f"{STORE}:{text.count(chr(10), 0, m.start()) + 1}")


def scan_hotkeys(catalog):
    text = open(os.path.join(ROOT, HOTKEY), encoding="utf-8").read()
    for m in re.finditer(r'=> \("\w+", "([^"]+)", "[^"]*"\)', text):
        catalog.add(m.group(1), None, f"{HOTKEY}:{text.count(chr(10), 0, m.start()) + 1}")


# ---------------------------------------------------------------------------------------------------------------
# Output


def po_quote(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"').replace("\t", "\\t").replace("\n", "\\n") + '"'


def po_field(keyword, s):
    if "\n" not in s[:-1] and len(keyword) + len(s) + 3 <= WRAP:
        return [f"{keyword} {po_quote(s)}"]
    parts = re.findall(r"[^\n]*\n|[^\n]+$", s)
    return [f'{keyword} ""'] + [po_quote(p) for p in parts]


def refs_lines(refs):
    lines, current = [], "#:"
    for ref in refs:
        if len(current) + 1 + len(ref) > WRAP and current != "#:":
            lines.append(current)
            current = "#:"
        current += " " + ref
    lines.append(current)
    return lines


def write(catalog, out):
    now = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%d %H:%M+0000")
    lines = [
        'msgid ""',
        'msgstr ""',
        '"Project-Id-Version: ThinkVision Control 0.1.0\\n"',
        f'"POT-Creation-Date: {now}\\n"',
        '"PO-Revision-Date: YEAR-MO-DA HO:MI+ZONE\\n"',
        '"Last-Translator: FULL NAME <EMAIL@ADDRESS>\\n"',
        '"Language-Team: LANGUAGE <LL@li.org>\\n"',
        '"MIME-Version: 1.0\\n"',
        '"Content-Type: text/plain; charset=UTF-8\\n"',
        '"Content-Transfer-Encoding: 8bit\\n"',
        '"Language: \\n"',
        '"Plural-Forms: nplurals=INTEGER; plural=EXPRESSION;\\n"',
        "",
    ]
    for e in catalog.entries.values():
        lines += refs_lines(e.refs)
        lines += po_field("msgid", e.msgid)
        if e.plural:
            lines += po_field("msgid_plural", e.plural)
            lines += ['msgstr[0] ""', 'msgstr[1] ""']
        else:
            lines.append('msgstr ""')
        lines.append("")
    with open(out, "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines))


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("-o", "--output", default=os.path.join(ROOT, "translations", "tvc.pot"))
    args = parser.parse_args()
    catalog = Catalog()
    for rel in rust_files():
        scan_rust(catalog, rel)
    scan_profiles(catalog)
    scan_store(catalog)
    scan_hotkeys(catalog)
    write(catalog, args.output)
    plurals = sum(1 for e in catalog.entries.values() if e.plural)
    print(f"{args.output}: {len(catalog.entries)} entries ({plurals} with plurals)")


if __name__ == "__main__":
    main()
