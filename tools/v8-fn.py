#!/usr/bin/env python3
"""Prints the V8 definition of a name, so a port can be read against it.

    tools/v8-fn.py Map::RawCopy
    tools/v8-fn.py MigrateSlowToFast
    tools/v8-fn.py --class LookupIterator
    tools/v8-fn.py --torque Map

Searches `<v8>/src` for a definition whose signature mentions the name and
prints it with its file and line, brace-matched. Pass a V8 checkout with
--v8 or set V8_ROOT; the default is ~/devel/v8.
"""
import argparse
import os
import re
import sys

EXTENSIONS = (".h", ".cc", ".tq")


def sources(root):
    for directory, _, files in os.walk(os.path.join(root, "src")):
        for name in sorted(files):
            if name.endswith(EXTENSIONS):
                yield os.path.join(directory, name)


def body_at(lines, index):
    """The brace-matched block starting at or after `lines[index]`."""
    depth = 0
    started = False
    out = []
    for line in lines[index:]:
        out.append(line)
        for character in line:
            if character == "{":
                depth += 1
                started = True
            elif character == "}":
                depth -= 1
        if started and depth <= 0:
            return out
        if len(out) > 400:
            return out
    return out


def signature_start(lines, index):
    """Walk back over a wrapped signature, template header and comments."""
    start = index
    while start > 0:
        previous = lines[start - 1].strip()
        if previous.startswith(("template", "//", "requires")) or previous.endswith(
            (",", "(", "&&", "||")
        ):
            start -= 1
            continue
        break
    return start


def find(root, pattern):
    hits = 0
    for path in sources(root):
        try:
            lines = open(path, errors="replace").read().split("\n")
        except OSError:
            continue
        for index, line in enumerate(lines):
            code = line.split("//")[0]
            if not pattern.search(code):
                continue
            if code.rstrip().endswith(";"):
                continue
            start = signature_start(lines, index)
            block = body_at(lines, start)
            if not any("{" in entry for entry in block):
                continue
            hits += 1
            relative = os.path.relpath(path, root)
            print(f"=== {relative}:{start + 1} ===")
            print("\n".join(block))
            print()
    return hits


def main():
    parser = argparse.ArgumentParser(add_help=True)
    parser.add_argument("name")
    parser.add_argument("--v8", default=os.environ.get("V8_ROOT", os.path.expanduser("~/devel/v8")))
    parser.add_argument("--class", dest="wanted_class", action="store_true")
    parser.add_argument("--torque", action="store_true")
    arguments = parser.parse_args()

    if not os.path.isdir(os.path.join(arguments.v8, "src")):
        sys.exit(f"usage: {sys.argv[0]} <Name> [--v8 <path-to-v8-checkout>]")

    name = arguments.name
    if arguments.torque:
        pattern = re.compile(r"^\s*(extern\s+)?class\s+" + re.escape(name) + r"\b")
    elif arguments.wanted_class:
        pattern = re.compile(
            r"^\s*(class|struct)\s+([A-Z0-9_]+\s+)?" + re.escape(name) + r"\b"
        )
    elif "::" in name:
        pattern = re.compile(re.escape(name) + r"\s*\(")
    else:
        # A definition at namespace scope, not a call: the signature starts at
        # column zero. An inline body inside a class needs Class::Method or
        # --class instead.
        pattern = re.compile(r"^\S.*?\b" + re.escape(name) + r"\s*\(")

    if find(arguments.v8, pattern) == 0:
        sys.exit(f"no definition of {name} found in {arguments.v8}/src")


main()
