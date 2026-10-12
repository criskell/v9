#!/usr/bin/env python3
"""Checks vx's bit field constants against V8's BitField chains.

V8 declares a bit field group as a chain:

    using EnumLengthBits = base::BitField<int32_t, 0, 10, uint32_t>;
    using NumberOfOwnDescriptorsBits = EnumLengthBits::Next<int32_t, 10>;
    using IsPrototypeMapBit = NumberOfOwnDescriptorsBits::Next<bool, 1>;

so every shift is determined by the one before it. This resolves the chains
and compares them with the `const NAME_SHIFT/_BIT/_BITS` declarations in
`src/`, which are the hand-written half and the easy thing to get wrong.

    tools/check-bitfields.py <v8-checkout>          # report mismatches
    tools/check-bitfields.py <v8-checkout> --emit Bits3
"""
import os
import re
import subprocess
import sys

CHAIN_HEAD = re.compile(
    r"using\s+(\w+)\s*=\s*base::BitField<\s*[\w:]+\s*,\s*(\d+)\s*,\s*(\d+)"
)
CHAIN_NEXT = re.compile(r"using\s+(\w+)\s*=\s*(\w+)::Next<\s*[\w:]+\s*,\s*(\d+)\s*>")
GROUP = re.compile(r"^\s*struct\s+(\w+)\s*:\s*BitFieldGroup")


def resolve(v8):
    """Every V8 bit field, as {(directory, field): (shift, size)}, and its groups."""
    fields = {}
    groups = {}
    chain = {}
    for directory, _, files in os.walk(os.path.join(v8, "src")):
        for name in sorted(files):
            if not name.endswith((".h", ".cc")):
                continue
            path = os.path.join(directory, name)
            where = os.path.relpath(directory, os.path.join(v8, "src"))
            group = None
            for line in open(path, errors="replace"):
                found = GROUP.match(line)
                if found:
                    group = found.group(1)
                head = CHAIN_HEAD.search(line)
                if head:
                    field, shift, size = head.group(1), int(head.group(2)), int(head.group(3))
                    chain[field] = (shift, size)
                    fields[(where, field)] = (shift, size)
                    groups.setdefault(group, []).append(field)
                    continue
                following = CHAIN_NEXT.search(line)
                if following:
                    field, previous, size = following.group(1), following.group(2), int(
                        following.group(3)
                    )
                    if previous not in chain:
                        continue
                    shift = chain[previous][0] + chain[previous][1]
                    chain[field] = (shift, size)
                    fields[(where, field)] = (shift, size)
                    groups.setdefault(group, []).append(field)
    return fields, groups, chain


def camel(name):
    """IS_DICTIONARY_MAP_BIT -> IsDictionaryMapBit, ENUM_LENGTH_BITS -> EnumLengthBits."""
    return "".join(part.capitalize() for part in name.split("_"))


def vx_constants():
    """Every `const NAME_(SHIFT|BIT|BITS): T = value;` in src/, by file."""
    pattern = re.compile(r"^const\s+([A-Z0-9_]+_(?:SHIFT|BIT|BITS))\s*:\s*\w+\s*=\s*(\d+)\s*;")
    out = []
    for path in subprocess.run(
        ["find", "src", "-name", "*.rs"], capture_output=True, text=True
    ).stdout.split():
        for number, line in enumerate(open(path), start=1):
            found = pattern.match(line)
            if found:
                out.append((path, number, found.group(1), int(found.group(2))))
    return out


def main():
    if len(sys.argv) < 2 or not os.path.isdir(os.path.join(sys.argv[1], "src")):
        sys.exit(f"usage: {sys.argv[0]} <path-to-v8-checkout> [--emit <Group>]")
    v8 = sys.argv[1]
    fields, groups, chain = resolve(v8)

    if "--emit" in sys.argv:
        group = sys.argv[sys.argv.index("--emit") + 1]
        if group not in groups:
            sys.exit(f"no bit field group named {group}; try one of {sorted(k for k in groups if k)}")
        for field in groups[group]:
            shift, size = chain[field]
            name = re.sub(r"(?<!^)(?=[A-Z])", "_", field).upper()
            if size == 1:
                print(f"const {name}: u32 = {shift};")
            else:
                stem = name[: -len("_BITS")]
                print(f"const {stem}_SHIFT: u32 = {shift};")
                print(f"const {stem}_BITS: u32 = {size};")
        return

    checked = 0
    bad = 0
    for path, number, name, value in vx_constants():
        where = os.path.dirname(path[len("src/"):])
        base = name.rsplit("_", 1)[0]
        kind = name.rsplit("_", 1)[1]
        candidates = [camel(name), camel(base) + "Bit", camel(base) + "Bits"]
        match = next((c for c in candidates if (where, c) in fields), None)
        if match is None:
            continue
        shift, size = fields[(where, match)]
        expected = shift if kind in ("SHIFT", "BIT") else size
        checked += 1
        if value != expected:
            bad += 1
            print(f"{path}:{number}: {name} is {value}, V8's {match} says {expected}")
    print(f"{bad} of {checked} bit field constants disagree with V8", file=sys.stderr)
    sys.exit(1 if bad else 0)


main()
