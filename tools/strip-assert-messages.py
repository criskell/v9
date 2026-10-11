#!/usr/bin/env python3
"""Drops the message argument from assertion macros in production code.

V8's DCHECK/DCHECK_EQ carry no message: the macro prints the expression and
the values. A message is prose, so it falls under the same rule as comments.

The three guards in `heap.rs` are kept: they are CHECK-class, they fire in
release builds, and the address they print is the whole point of them. V8's
equivalent is FATAL(), which does carry text.

  tools/strip-assert-messages.py src/foo.rs [...]
"""
import re, sys

MACROS = re.compile(r"\b(debug_assert|assert)(_eq|_ne)?!\(")

def split_args(text):
    """Top-level comma positions inside a macro's argument list."""
    depth = 0
    in_string = False
    escaped = False
    commas = []
    for index, char in enumerate(text):
        if in_string:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
            continue
        if char == '"':
            in_string = True
        elif char in "([{":
            depth += 1
        elif char in ")]}":
            if depth == 0:
                return commas, index
            depth -= 1
        elif char == "," and depth == 0:
            commas.append(index)
    return commas, len(text)

for path in sys.argv[1:]:
    source = open(path).read()
    body, _, tests = source.partition("#[cfg(test)]")
    out = []
    position = 0
    dropped = 0
    while True:
        match = MACROS.search(body, position)
        if not match:
            out.append(body[position:])
            break
        start = match.end()
        commas, close = split_args(body[start:])
        args = body[start : start + close]
        if commas:
            keep = 1 if match.group(2) else 0
            if len(commas) > keep:
                cut = commas[keep]
                tail = args[cut + 1 :].strip()
                if tail.startswith('"'):
                    out.append(body[position:start])
                    out.append(args[:cut].rstrip())
                    out.append(body[start + close :][:1])
                    position = start + close + 1
                    dropped += 1
                    continue
        out.append(body[position : start + close + 1])
        position = start + close + 1
    open(path, "w").write("".join(out) + ("#[cfg(test)]" if tests else "") + tests)
    print(f"{path}: dropped {dropped}")
