#!/usr/bin/env python3
"""Rewrites a V8 definition into a Rust draft, mechanically.

    tools/v8-draft.py LookupIterator::Delete

The output does not compile. It preserves statement order and converts the
spellings that are almost always the same conversion, so what is left to do
is the part that needs judgement. Anything the rules do not recognise is
passed through unchanged, so it is visible.
"""
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))


def snake(name):
    name = re.sub(r"(.)([A-Z][a-z]+)", r"\1_\2", name)
    name = re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", name)
    return name.lower()


def constant(name):
    return "K_" + snake(name[1:]).upper()


CALL = re.compile(r"\b([A-Z]\w*)::([A-Z]\w*)\s*\(")
METHOD = re.compile(r"(->|\.)([A-Z]\w*)\s*\(")
FIELD = re.compile(r"\b(\w+)_\b")
CONSTANT = re.compile(r"\bk([A-Z]\w*)\b")
TEMPLATE_CAST = re.compile(r"\bCast<\w+>\(([^()]*)\)")
HANDLE_DECL = re.compile(
    r"^(\s*)(?:Direct|Indirect)?Handle<\w+>\s+(\w+)\s*\(\s*([^;]*?)\s*,\s*isolate_?\s*\)\s*;"
)
TYPED_DECL = re.compile(
    r"^(\s*)(?:const\s+)?[\w:]+(?:<[^<>]*>)?\s*[*&]?\s+(\w+)\s*=\s*(?!=)"
)
TERNARY = re.compile(r"^(\s*)let (\w+) = (.+?)\s*\?\s*(.+?)\s*:\s*(.+);$")
FOR_RANGE = re.compile(r"for\s*\(\s*\w+\s+(\w+)\s*:\s*\w+::Range\(([^)]*)\)\s*\)")
FOR_COUNT = re.compile(r"for\s*\(\s*int\s+(\w+)\s*=\s*0\s*;\s*\w+\s*<\s*([^;]+);\s*\+\+?\w+\+?\+?\s*\)")
IF_WHILE = re.compile(r"\b(if|while)\s*\((.*)\)\s*\{\s*$")

DROP = re.compile(r"^\s*(RCS_SCOPE|DisallowGarbageCollection|DisallowHeapAllocation|HandleScope|//)")

LITERAL = {
    "UNREACHABLE()": "unreachable!()",
    "true": "true",
    "false": "false",
    "nullptr": "None",
}


def rewrite(line):
    if DROP.match(line):
        return None
    indent = re.match(r"\s*", line).group(0)
    body = line.strip()

    if body in ("{", "}", "} else {", "else {"):
        return line.rstrip()

    body = re.sub(r"\bDCHECK_EQ\(([^,]+),\s*(.+)\);", r"debug_assert_eq!(\1, \2);", body)
    body = re.sub(r"\bDCHECK_NE\(([^,]+),\s*(.+)\);", r"debug_assert_ne!(\1, \2);", body)
    body = re.sub(r"\bDCHECK\((.+)\);", r"debug_assert!(\1);", body)
    body = re.sub(r"\bCHECK\((.+)\);", r"assert!(\1);", body)
    body = body.replace("UNREACHABLE()", "unreachable!()")

    found = HANDLE_DECL.match(indent + body)
    if found:
        return f"{found.group(1)}let {found.group(2)} = {found.group(3)};"

    found = FOR_RANGE.search(body)
    if found:
        body = FOR_RANGE.sub(rf"for {found.group(1)} in 0..{found.group(2)} {{", body)
    found = FOR_COUNT.search(body)
    if found:
        body = FOR_COUNT.sub(rf"for {found.group(1)} in 0..{found.group(2)} {{", body)

    found = IF_WHILE.match(body)
    if found:
        body = f"{found.group(1)} {found.group(2)} {{"

    body = TEMPLATE_CAST.sub(r"\1", body)
    body = re.sub(r"\*(\w+)\b", r"\1", body)
    body = body.replace("isolate_->factory()->", "self.")
    body = body.replace("isolate()->factory()->", "self.")
    body = body.replace("isolate_->", "self.")
    body = body.replace("isolate(), ", "")
    body = body.replace("isolate_, ", "")
    body = body.replace("isolate_", "self")

    body = CALL.sub(lambda m: f"self.{snake(m.group(2))}(", body)
    body = METHOD.sub(lambda m: f".{snake(m.group(2))}(", body)
    body = CONSTANT.sub(lambda m: constant("k" + m.group(1)), body)
    body = body.replace("->", ".")
    body = FIELD.sub(lambda m: f"self.{m.group(1)}", body)

    body = TYPED_DECL.sub(r"\1let \2 = ", indent + body).strip()
    line = indent + body
    found = TERNARY.match(line)
    if found:
        return (
            f"{found.group(1)}let {found.group(2)} = "
            f"if {found.group(3)} {{ {found.group(4)} }} else {{ {found.group(5)} }};"
        )
    return line


SIGNATURE = re.compile(
    r"^\s*(?:[\w:<>*&]+\s+)+?(?:(\w+)::)?(\w+)\(([^)]*)\)\s*(?:const\s*)?\{\s*$"
)


def join_continuations(lines):
    """A C++ statement split over several lines becomes one line."""
    out = []
    pending = ""
    for line in lines:
        stripped = line.strip()
        if stripped.startswith("==="):
            if pending:
                out.append(pending)
                pending = ""
            out.append(line)
            continue
        if not stripped:
            if pending:
                out.append(pending)
                pending = ""
            out.append(line)
            continue
        if pending:
            pending = pending.rstrip() + " " + stripped
        else:
            pending = line.rstrip()
        if pending.rstrip().endswith((";", "{", "}", ":")) or stripped.startswith("//"):
            out.append(pending)
            pending = ""
    if pending:
        out.append(pending)
    return out


def rewrite_signature(line):
    found = SIGNATURE.match(line)
    if not found:
        return None
    owner, name, arguments = found.group(1), found.group(2), found.group(3)
    parameters = []
    for argument in arguments.split(","):
        argument = argument.strip()
        if not argument or "isolate" in argument.lower():
            continue
        words = argument.replace("*", " ").replace("&", " ").split()
        if len(words) >= 2:
            parameters.append(f"{words[-1]}: {' '.join(words[:-1])}")
    receiver = "&mut self"
    head = f"fn {snake(name)}({', '.join([receiver] + parameters)})"
    return head + " {" if owner else head + " {"


def main():
    if len(sys.argv) < 2:
        sys.exit(f"usage: {sys.argv[0]} <Name> [--v8 <path>]")
    found = subprocess.run(
        [sys.executable, os.path.join(HERE, "v8-fn.py")] + sys.argv[1:],
        capture_output=True,
        text=True,
    )
    if found.returncode != 0:
        sys.exit(found.stderr.strip())
    for line in join_continuations(found.stdout.split("\n")):
        if line.startswith("==="):
            print(line)
            continue
        signature = rewrite_signature(line)
        if signature is not None:
            print(signature)
            continue
        out = rewrite(line)
        if out is not None:
            print(out)


main()
