#!/usr/bin/env python3
"""Checks that `unsafe` only appears where the policy allows it.

The port is safe Rust with `unsafe` quarantined: a small, named set of
modules that own the operations Rust cannot verify — mapping executable
memory, calling into generated code, mapping a serialized heap, the embedder
contract, and the platform layer. Everything else must be free of it.

    tools/check-unsafe.py

Reports every `unsafe` outside the allowlist, and the total, so growth is
visible. A file is added to ALLOWED only with a reason.
"""
import os
import re
import subprocess
import sys

# Each entry is a module that owns an operation Rust cannot verify, and the
# V8 file whose job it is doing.
ALLOWED = {
    # "src/base/platform/platform.rs": "OS::Allocate, page permissions",
    # "src/heap/code_range.rs": "executable pages for generated code",
    # "src/execution/simulator.rs": "GeneratedCode::Call, the jump into code",
    # "src/snapshot/snapshot.rs": "a serialized heap mapped into memory",
    # "src/sandbox/sandbox.rs": "the sandbox reservation itself",
    # "src/api/api.rs": "the embedder contract",
}

UNSAFE = re.compile(r"\bunsafe\b")


def main():
    total = 0
    outside = 0
    for path in sorted(
        subprocess.run(
            ["find", "src", "-name", "*.rs"], capture_output=True, text=True
        ).stdout.split()
    ):
        for number, line in enumerate(open(path), start=1):
            if line.lstrip().startswith("//"):
                continue
            if not UNSAFE.search(line):
                continue
            total += 1
            if path in ALLOWED:
                continue
            outside += 1
            print(f"{path}:{number}: unsafe outside the allowlist")
    print(
        f"{outside} of {total} uses of unsafe are outside the allowlist "
        f"({len(ALLOWED)} files allowed)",
        file=sys.stderr,
    )
    sys.exit(1 if outside else 0)


main()
